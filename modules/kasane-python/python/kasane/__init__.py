"""Kasane 2D authoring SDK. Paths passed to native IO must be absolute."""

from __future__ import annotations

from pathlib import Path
from typing import Mapping, NamedTuple, Sequence
from weakref import WeakSet

from ._native import NativeSession, ObjectHandle, SdkFailure, capabilities

Version = tuple[int, int, int]
Point = tuple[float, float]


class Appearance(NamedTuple):
    opacity: float = 1
    multiply: tuple[float, float, float] = (1, 1, 1)
    screen: tuple[float, float, float] = (0, 0, 0)


class MeshSnapshot(NamedTuple):
    id: str
    name: str
    vertex_ids: list[int]
    positions: list[Point]
    version: Version


class MeshProperties(NamedTuple):
    texture_asset_id: str
    appearance: Appearance
    draw_order: float | None
    blend_mode: str
    enabled: bool
    double_sided: bool
    inverted_mask: bool
    masks: list[str]


class MeshPropertiesSnapshot(NamedTuple):
    texture_asset_id: str
    appearance: Appearance
    draw_order: float | None
    blend_mode: str
    enabled: bool
    double_sided: bool
    inverted_mask: bool
    masks: list[str]
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


class DrawOrderGroup(NamedTuple):
    owner: str
    items: list[str]
    min_order: int
    max_order: int


class GeometrySnapshot(NamedTuple):
    version: Version
    mesh_id: str
    vertex_ids: list[int]
    positions: list[Point]
    uvs: list[Point]
    triangles: list[tuple[int, int, int]]
    space: str
    parent_id: str | None


class GeometryIssue(NamedTuple):
    kind: str
    mesh_id: str
    triangle_index: int | None


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
    appearance: Appearance = Appearance()
    draw_order: float | None = None


class MeshBindingSnapshot(NamedTuple):
    id: str
    mesh_id: str
    axes: list[Axis]
    keyforms: list[MeshKeyform]
    version: Version


class PartSnapshot(NamedTuple):
    id: str
    runtime_id: str
    name: str
    parent_id: str
    enabled: bool
    draw_order: float
    version: Version


class RotationPose(NamedTuple):
    origin: tuple[float, float]
    angle: float = 0
    scale: float = 1
    reflect_x: bool = False
    reflect_y: bool = False


class SceneWarpKeyform(NamedTuple):
    keys: list[float]
    positions: list[Point]
    appearance: Appearance = Appearance()


class SceneRotationKeyform(NamedTuple):
    keys: list[float]
    rotation: RotationPose
    appearance: Appearance = Appearance()


class ScenePartKeyform(NamedTuple):
    keys: list[float]
    draw_order: float


SceneKeyform = SceneWarpKeyform | SceneRotationKeyform | ScenePartKeyform


class SceneBindingSnapshot(NamedTuple):
    id: str
    axes: list[Axis]
    kind: str
    target_id: str
    keyforms: list[SceneKeyform]
    version: Version


class RotationData(NamedTuple):
    base_angle: float
    pose: RotationPose


class WarpData(NamedTuple):
    rows: int
    columns: int
    quad: bool
    points: list[Point]


class TransformSnapshot(NamedTuple):
    id: str
    runtime_id: str
    name: str
    part_id: str | None
    parent_id: str | None
    kind: str
    rotation: RotationData | None
    warp: WarpData | None
    enabled: bool
    appearance: Appearance
    version: Version


class OffscreenKeyform(NamedTuple):
    opacity: float
    multiply: tuple[float, float, float] | None = None
    screen: tuple[float, float, float] | None = None


class OffscreenSpec(NamedTuple):
    id: str
    name: str
    part_id: str
    blend_mode: int = 0
    flags: int = 4
    masks: Sequence[str] = ()
    part_keyform_indices: Sequence[int] = ()
    keyforms: Sequence[OffscreenKeyform] = ()


class OffscreenSnapshot(NamedTuple):
    id: str
    runtime_id: str
    name: str
    part_id: str
    blend_mode: int
    flags: int
    masks: list[str]
    part_keyform_indices: list[int]
    keyforms: list[OffscreenKeyform]
    version: Version


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

    def add_png_asset_from_base(
        self, asset_id: str, name: str, absolute_base: Path, relative_path: Path
    ) -> None:
        self._call(lambda: self._native.add_png_asset_from_base(
            asset_id, name, str(absolute_base), str(relative_path)
        ))

    def replace_png_asset(self, asset_id: str, name: str, absolute_path: Path) -> None:
        self._call(lambda: self._native.replace_png_asset(asset_id, name, str(absolute_path)))

    def relocate_png_asset(self, asset_id: str, absolute_path: Path) -> None:
        self._call(lambda: self._native.relocate_png_asset(asset_id, str(absolute_path)))

    def replace_canvas(
        self, width: float, height: float, origin: Point, pixels_per_unit: float
    ) -> None:
        self._call(
            lambda: self._native.replace_canvas(
                width, height, origin[0], origin[1], pixels_per_unit
            )
        )

    def replace_draw_order_groups(self, groups: Sequence[DrawOrderGroup]) -> None:
        self._call(
            lambda: self._native.replace_draw_order_groups(
                [(g.owner, list(g.items), g.min_order, g.max_order) for g in groups]
            )
        )

    def create_part(
        self,
        part_id: str,
        name: str,
        parent_id: str = "",
        enabled: bool = True,
        draw_order: float = 0,
    ) -> None:
        self._call(
            lambda: self._native.create_part(
                part_id, name, parent_id, enabled, draw_order
            )
        )

    def replace_part(
        self, part_id: str, name: str, parent_id: str, enabled: bool, draw_order: float
    ) -> None:
        self._call(
            lambda: self._native.replace_part(
                part_id, name, parent_id, enabled, draw_order
            )
        )

    def create_rotation_transform(
        self, transform_id: str, name: str, rotation: RotationData,
        part_id: str | None = None, parent_id: str | None = None,
    ) -> None:
        self._call(lambda: self._native.create_rotation_transform(
            transform_id, name, part_id, parent_id, _rotation_tuple(rotation)
        ))

    def create_warp_transform(
        self, transform_id: str, name: str, warp: WarpData,
        part_id: str | None = None, parent_id: str | None = None,
    ) -> None:
        self._call(lambda: self._native.create_warp_transform(
            transform_id, name, part_id, parent_id, warp.rows, warp.columns,
            warp.quad, list(warp.points)
        ))

    def update_rotation(self, transform_id: str, rotation: RotationData) -> None:
        self._call(lambda: self._native.update_rotation(transform_id, _rotation_tuple(rotation)))

    def replace_transform(self, transform: TransformSnapshot) -> None:
        rotation = _rotation_tuple(transform.rotation) if transform.rotation is not None else None
        warp = (
            (transform.warp.rows, transform.warp.columns, transform.warp.quad, list(transform.warp.points))
            if transform.warp is not None else None
        )
        self._call(lambda: self._native.replace_transform(
            transform.id, transform.name, transform.part_id, transform.parent_id,
            transform.kind, rotation, warp, transform.enabled, tuple(transform.appearance),
        ))

    def create_offscreen(self, offscreen: OffscreenSpec) -> None:
        self._call(lambda: self._native.create_offscreen(_offscreen_data(offscreen)))

    def replace_offscreen(self, offscreen: OffscreenSnapshot) -> None:
        self._call(lambda: self._native.replace_offscreen(_offscreen_data(offscreen)))

    def replace_part_binding_with_offscreen(
        self, binding: SceneBindingSnapshot, offscreen: OffscreenSnapshot,
    ) -> None:
        if binding.kind != "part":
            self._native.abort()
            raise ValueError("Expected a Part scene binding")
        self._call(lambda: self._native.replace_part_binding_with_offscreen(
            binding.id, binding.target_id,
            [(axis.parameter_id, list(axis.keys)) for axis in binding.axes],
            [_scene_form_tuple("part", form) for form in binding.keyforms],
            _offscreen_data(offscreen),
        ))

    def update_warp_points(self, transform_id: str, points: Sequence[Point]) -> None:
        self._call(lambda: self._native.update_warp_points(transform_id, list(points)))

    def erase_object(self, object_id: str) -> None:
        self._call(lambda: self._native.erase_object(object_id))

    def replace_parameter(
        self,
        parameter_id: str,
        name: str,
        minimum: float,
        maximum: float,
        default_value: float,
        repeat: bool = False,
    ) -> None:
        self._call(
            lambda: self._native.replace_parameter(
                parameter_id, name, minimum, maximum, default_value, repeat
            )
        )

    def set_organization_parent(self, part_id: str, parent_id: str) -> None:
        self._call(lambda: self._native.set_organization_parent(part_id, parent_id))

    def set_transform_parent(self, transform_id: str, parent_id: str | None) -> None:
        self._call(lambda: self._native.set_transform_parent(transform_id, parent_id))

    def set_transform_part(self, transform_id: str, part_id: str | None) -> None:
        self._call(lambda: self._native.set_transform_part(transform_id, part_id))

    def set_deform_parent(self, mesh_id: str, transform_id: str) -> None:
        self._call(lambda: self._native.set_deform_parent(mesh_id, transform_id))

    def set_mesh_part(self, mesh_id: str, part_id: str) -> None:
        self._call(lambda: self._native.set_mesh_part(mesh_id, part_id))

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
                [_mesh_form_tuple(form) for form in forms],
            )
        )

    def replace_mesh_binding(
        self, binding_id: str, mesh_id: str,
        axes: Sequence[Axis], forms: Sequence[MeshKeyform],
    ) -> None:
        self._call(lambda: self._native.replace_mesh_binding(
            binding_id, mesh_id,
            [(axis.parameter_id, list(axis.keys)) for axis in axes],
            [_mesh_form_tuple(form) for form in forms],
        ))

    def set_mesh_keyform(self, binding_id: str, form: MeshKeyform) -> None:
        self._call(lambda: self._native.set_mesh_keyform(
            binding_id, _mesh_form_tuple(form)
        ))

    def update_mesh_properties(self, mesh_id: str, properties: MeshProperties) -> None:
        self._call(lambda: self._native.update_mesh_properties(
            mesh_id,
            properties.texture_asset_id,
            tuple(properties.appearance),
            properties.draw_order,
            properties.blend_mode,
            properties.enabled,
            properties.double_sided,
            properties.inverted_mask,
            list(properties.masks),
        ))

    def create_scene_binding(
        self, binding_id: str, kind: str, target_id: str,
        axes: Sequence[Axis], forms: Sequence[SceneKeyform],
    ) -> None:
        self._call(lambda: self._native.create_scene_binding(
            binding_id, kind, target_id,
            [(axis.parameter_id, list(axis.keys)) for axis in axes],
            [_scene_form_tuple(kind, form) for form in forms],
        ))

    def replace_scene_binding(
        self, binding_id: str, kind: str, target_id: str,
        axes: Sequence[Axis], forms: Sequence[SceneKeyform],
    ) -> None:
        self._call(lambda: self._native.replace_scene_binding(
            binding_id, kind, target_id,
            [(axis.parameter_id, list(axis.keys)) for axis in axes],
            [_scene_form_tuple(kind, form) for form in forms],
        ))

    def set_scene_keyform(self, binding_id: str, form: SceneKeyform) -> None:
        self._call(lambda: self._native.set_scene_keyform(
            binding_id, _scene_kind(form), _scene_form_tuple(_scene_kind(form), form)
        ))

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

    @classmethod
    def with_history_limits(
        cls,
        document_id: str,
        width: float,
        height: float,
        origin: Point,
        pixels_per_unit: float,
        max_steps: int,
        max_bytes: int,
    ) -> Session:
        native = NativeSession.with_history_limits(
            document_id,
            width,
            height,
            origin[0],
            origin[1],
            pixels_per_unit,
            max_steps,
            max_bytes,
        )
        return cls._from_native(native)

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

    def mesh_properties(self, mesh_id: str) -> MeshPropertiesSnapshot | None:
        raw = self._native.mesh_properties(mesh_id)
        if raw is None:
            return None
        texture_asset_id, appearance, draw_order, blend_mode, enabled, double_sided, inverted_mask, masks, version = raw
        return MeshPropertiesSnapshot(
            texture_asset_id, Appearance(*appearance), draw_order, blend_mode,
            enabled, double_sided, inverted_mask, masks, version,
        )

    def binding(self, binding_id: str) -> MeshBindingSnapshot | None:
        return _binding_snapshot(self._native.binding(binding_id))

    def binding_for_mesh(self, mesh_id: str) -> MeshBindingSnapshot | None:
        return _binding_snapshot(self._native.binding_for_mesh(mesh_id))

    def scene_binding(self, binding_id: str) -> SceneBindingSnapshot | None:
        return _scene_binding_snapshot(self._native.scene_binding(binding_id))

    def binding_for_scene(self, target_id: str) -> SceneBindingSnapshot | None:
        return _scene_binding_snapshot(self._native.binding_for_scene(target_id))

    def part(self, part_id: str) -> PartSnapshot | None:
        raw = self._native.part(part_id)
        return PartSnapshot(*raw) if raw is not None else None

    def transform(self, transform_id: str) -> TransformSnapshot | None:
        raw = self._native.transform(transform_id)
        if raw is None:
            return None
        id, runtime_id, name, part_id, parent_id, kind, rotation, warp, enabled, appearance, version = raw
        rotation_data = RotationData(rotation[0], RotationPose((rotation[1][0], rotation[1][1]), *rotation[1][2:])) if rotation is not None else None
        warp_data = WarpData(*warp) if warp is not None else None
        return TransformSnapshot(id, runtime_id, name, part_id, parent_id, kind, rotation_data, warp_data, enabled, Appearance(*appearance), version)

    def offscreen(self, offscreen_id: str) -> OffscreenSnapshot | None:
        raw = self._native.offscreen(offscreen_id)
        if raw is None:
            return None
        id, runtime_id, name, part_id, blend_mode, flags, masks, indices, forms, version = raw
        return OffscreenSnapshot(
            id, runtime_id, name, part_id, blend_mode, flags, masks, indices,
            [OffscreenKeyform(*form) for form in forms], version,
        )

    def handle(self, kind: str, object_id: str) -> ObjectHandle:
        return self._native.handle(kind, object_id)

    def resolve_handle(self, handle: ObjectHandle) -> None:
        self._native.resolve_handle(handle)

    def mesh_by_handle(self, handle: ObjectHandle) -> MeshSnapshot:
        return MeshSnapshot(*self._native.mesh_by_handle(handle))

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

    def diagnose_geometry(
        self,
        min_triangle_area: float = 0,
        canvas_bounds: tuple[Point, Point] | None = None,
    ) -> list[GeometryIssue]:
        return [
            GeometryIssue(*item)
            for item in self._native.diagnose_geometry(min_triangle_area, canvas_bounds)
        ]

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
    def draw_order_groups(self) -> list[DrawOrderGroup] | None:
        raw = self._native.draw_order_groups()
        return [DrawOrderGroup(*group) for group in raw] if raw is not None else None

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


def _binding_snapshot(raw) -> MeshBindingSnapshot | None:
    if raw is None:
        return None
    binding_id, mesh_id, axes, forms, version = raw
    return MeshBindingSnapshot(
        binding_id,
        mesh_id,
        [Axis(parameter_id, keys) for parameter_id, keys in axes],
        [MeshKeyform(keys, positions, Appearance(*appearance), draw_order)
         for keys, positions, appearance, draw_order in forms],
        version,
    )


def _mesh_form_tuple(form: MeshKeyform):
    return (
        list(form.keys), list(form.positions), tuple(form.appearance), form.draw_order
    )


def _offscreen_data(value: OffscreenSpec | OffscreenSnapshot):
    return (
        value.id, value.name, value.part_id, value.blend_mode, value.flags,
        list(value.masks), list(value.part_keyform_indices),
        [(form.opacity, form.multiply, form.screen) for form in value.keyforms],
    )


def _rotation_tuple(rotation: RotationData):
    pose = rotation.pose
    return (rotation.base_angle, (
        pose.origin[0], pose.origin[1], pose.angle, pose.scale,
        pose.reflect_x, pose.reflect_y,
    ))


def _scene_kind(form: SceneKeyform) -> str:
    if isinstance(form, SceneWarpKeyform):
        return "warp"
    if isinstance(form, SceneRotationKeyform):
        return "rotation"
    if isinstance(form, ScenePartKeyform):
        return "part"
    raise TypeError("Unsupported scene keyform type")


def _scene_form_tuple(kind: str, form: SceneKeyform):
    if _scene_kind(form) != kind:
        raise TypeError("Scene keyform does not match track kind")
    if isinstance(form, SceneWarpKeyform):
        return (list(form.keys), list(form.positions), None, None, tuple(form.appearance))
    if isinstance(form, SceneRotationKeyform):
        pose = form.rotation
        return (list(form.keys), [], (
            pose.origin[0], pose.origin[1], pose.angle, pose.scale,
            pose.reflect_x, pose.reflect_y,
        ), None, tuple(form.appearance))
    return (list(form.keys), [], None, form.draw_order, None)


def _scene_binding_snapshot(raw) -> SceneBindingSnapshot | None:
    if raw is None:
        return None
    binding_id, axes, kind, target_id, forms, version = raw
    keyforms: list[SceneKeyform] = []
    for keys, positions, pose, draw_order, appearance in forms:
        if kind == "warp":
            keyforms.append(SceneWarpKeyform(keys, positions, Appearance(*appearance)))
        elif kind == "rotation":
            rotation = RotationPose((pose[0], pose[1]), *pose[2:])
            keyforms.append(SceneRotationKeyform(keys, rotation, Appearance(*appearance)))
        else:
            keyforms.append(ScenePartKeyform(keys, draw_order))
    return SceneBindingSnapshot(
        binding_id,
        [Axis(parameter_id, keys) for parameter_id, keys in axes],
        kind,
        target_id,
        keyforms,
        version,
    )


__all__ = [
    "Axis",
    "Appearance",
    "AssetSnapshot",
    "CanvasSnapshot",
    "DrawableSample",
    "DrawOrderGroup",
    "Edit",
    "EditEvent",
    "Evaluation",
    "ExportResult",
    "GeometrySnapshot",
    "GeometryIssue",
    "HistoryState",
    "ImportResult",
    "MeshSnapshot",
    "MeshProperties",
    "MeshPropertiesSnapshot",
    "MeshBindingSnapshot",
    "MeshKeyform",
    "ObjectHandle",
    "OffscreenKeyform",
    "OffscreenSpec",
    "OffscreenSnapshot",
    "ParameterSample",
    "ParameterSnapshot",
    "PartSnapshot",
    "ResourceIssue",
    "SceneBindingSnapshot",
    "ScenePartKeyform",
    "SceneRotationKeyform",
    "SceneWarpKeyform",
    "RotationData",
    "RotationPose",
    "SaveResult",
    "SdkFailure",
    "Session",
    "StructureIssue",
    "TransformSnapshot",
    "WarpData",
    "capabilities",
    "open_project",
]
