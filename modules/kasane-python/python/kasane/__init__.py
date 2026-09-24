"""Kasane 2D authoring SDK.

Paths passed to native IO must be absolute. Coordinates are ordinary ``(x, y)``
tuples; ``Point`` is a type alias, not a two-argument constructor.
"""

from __future__ import annotations

import hashlib
from importlib.metadata import version as package_version
import json
import math
from pathlib import Path
import platform
import struct
from typing import Mapping, NamedTuple, Sequence
from uuid import UUID, uuid4
from weakref import WeakSet
import zlib

from ._native import NativeSession, ObjectHandle, SdkFailure, capabilities
from . import _native as _native_module
try:
    from ._native import NativeObserver, ObservationFailure
except ImportError:
    NativeObserver = None
    class ObservationFailure(Exception):
        """Raised only by wheels built with the observe feature."""

Version = tuple[int, int, int]
Point = tuple[float, float]


class Appearance(NamedTuple):
    """Drawable opacity and RGB multiply/screen colors."""

    opacity: float = 1
    multiply: tuple[float, float, float] = (1, 1, 1)
    screen: tuple[float, float, float] = (0, 0, 0)


class MeshSnapshot(NamedTuple):
    """Copy of a mesh's name, vertex IDs, and source positions."""

    id: str
    name: str
    vertex_ids: list[int]
    positions: list[Point]
    version: Version


class MeshProperties(NamedTuple):
    """Drawing fields accepted by Edit.update_mesh_properties."""

    texture_asset_id: str
    appearance: Appearance
    draw_order: float | None
    blend_mode: str
    enabled: bool
    double_sided: bool
    inverted_mask: bool
    masks: list[str]


class MeshPropertiesSnapshot(NamedTuple):
    """Versioned copy of a mesh's drawing fields."""

    texture_asset_id: str
    appearance: Appearance
    draw_order: float | None
    blend_mode: str
    enabled: bool
    double_sided: bool
    inverted_mask: bool
    masks: list[str]
    version: Version


class MeshGeometryData(NamedTuple):
    """Source vertices, UVs, and triangles indexed by vertex ID."""

    vertex_ids: Sequence[int]
    positions: Sequence[Point]
    uvs: Sequence[Point]
    triangles: Sequence[tuple[int, int, int]]


class MeshDrawingData(NamedTuple):
    """Texture, appearance, ordering, blending, and mask settings for a mesh."""

    texture_asset_id: str
    appearance: Appearance = Appearance()
    draw_order: float | None = None
    blend_mode: str = "normal"
    enabled: bool = True
    double_sided: bool = True
    inverted_mask: bool = False
    masks: Sequence[str] = ()
    raw_blend_mode: int | None = None


class MeshRecordSpec(NamedTuple):
    """Complete input record for creating a mesh."""

    id: str
    name: str
    geometry: MeshGeometryData
    drawing: MeshDrawingData
    part_id: str = ""
    deformer_id: str = ""


class MeshRecordSnapshot(NamedTuple):
    """Versioned full mesh record, including runtime and parent identities."""

    id: str
    runtime_id: str
    name: str
    geometry: MeshGeometryData
    drawing: MeshDrawingData
    part_id: str
    deformer_id: str
    version: Version


class TextureRevision(NamedTuple):
    """Content hash and GPU upload revision of an observed texture."""

    asset_id: str
    sha256: str
    revision: int


class DrawableBounds(NamedTuple):
    """Visibility and optional pixel bounds of a rendered drawable."""

    id: str
    visible: bool
    bounds: tuple[int, int, int, int] | None


class ObservedFrame(NamedTuple):
    """Rendered RGBA/PNG image with input, version, view, and adapter details."""

    version: Version
    input_sha256: str
    evaluation_revision: int
    document_id: str
    source_revision: int
    parameters: list[ParameterSample]
    canvas: CanvasSnapshot
    view_scale: float
    view_offset: Point
    drawable_bounds: list[DrawableBounds]
    width: int
    height: int
    rgba: bytes
    png: bytes
    texture_revisions: list[TextureRevision]
    adapter_name: str
    adapter_backend: str

    def save_png(self, path: Path) -> None:
        """Write this frame's PNG bytes to an absolute path."""

        if not path.is_absolute():
            raise ValueError("PNG output path must be absolute")
        path.write_bytes(self.png)


class ObservationRun(NamedTuple):
    """Paths to one observation run's report, images, and contact sheet."""

    directory: Path
    report: Path
    frames: list[Path]
    crops: list[Path]
    contact_sheet: Path

    @property
    def output(self) -> Path:
        """The unique output directory created for this observation run."""
        return self.directory


class AssetSnapshot(NamedTuple):
    """Versioned PNG asset metadata, including source path and SHA-256."""

    id: str
    name: str
    source: str
    width: int
    height: int
    sha256: str
    version: Version


class CanvasSnapshot(NamedTuple):
    """Canvas pixel size, origin, and pixels-per-runtime-unit scale."""

    width: float
    height: float
    origin_x: float
    origin_y: float
    pixels_per_unit: float

    @property
    def origin(self) -> Point:
        """Canvas origin as the same ``(x, y)`` pair accepted by Session."""
        return (self.origin_x, self.origin_y)


class DrawOrderGroup(NamedTuple):
    """Explicit draw-order range and ordered items under an owner."""

    owner: str
    items: list[str]
    min_order: int
    max_order: int


class GeometrySnapshot(NamedTuple):
    """Versioned source geometry; ``space`` identifies canvas or parent coordinates."""

    version: Version
    mesh_id: str
    vertex_ids: list[int]
    positions: list[Point]
    uvs: list[Point]
    triangles: list[tuple[int, int, int]]
    space: str
    parent_id: str | None


class GeometryIssue(NamedTuple):
    """Non-blocking authoring hint for a mesh or triangle."""

    kind: str
    mesh_id: str
    triangle_index: int | None


class HistoryState(NamedTuple):
    """Undo/redo counts, estimated use, and configured history limits."""

    undo_steps: int
    redo_steps: int
    estimated_bytes: int
    max_steps: int
    max_bytes: int


class EditEvent(NamedTuple):
    """Published change event with before/after versions and affected IDs."""

    label: str
    before: Version
    after: Version
    kind: str
    object_ids: list[str]
    changed: bool


class ParameterSnapshot(NamedTuple):
    """Versioned parameter definition and its sampling range."""

    id: str
    runtime_id: str
    name: str
    minimum: float
    maximum: float
    default_value: float
    repeat: bool
    kind: str
    version: Version


class Axis(NamedTuple):
    """Parameter ID and ordered key values for a binding axis."""

    parameter_id: str
    keys: list[float]


class MeshKeyform(NamedTuple):
    """Mesh key combination, positions, and appearance; use ``_replace`` to copy."""

    keys: list[float]
    positions: list[Point]
    appearance: Appearance = Appearance()
    draw_order: float | None = None


class MeshBindingSnapshot(NamedTuple):
    """Versioned mesh binding with axes and a complete keyform grid."""

    id: str
    mesh_id: str
    axes: list[Axis]
    keyforms: list[MeshKeyform]
    version: Version


class PartSnapshot(NamedTuple):
    """Versioned Part and its organization parent and drawing state."""

    id: str
    runtime_id: str
    name: str
    parent_id: str
    enabled: bool
    draw_order: float
    version: Version


class RotationPose(NamedTuple):
    """Rotation origin, angle, scale, and reflection flags."""

    origin: tuple[float, float]
    angle: float = 0
    scale: float = 1
    reflect_x: bool = False
    reflect_y: bool = False


class SceneWarpKeyform(NamedTuple):
    """Warp scene keyform with control points and appearance."""

    keys: list[float]
    positions: list[Point]
    appearance: Appearance = Appearance()


class SceneRotationKeyform(NamedTuple):
    """Rotation scene keyform with pose and appearance."""

    keys: list[float]
    rotation: RotationPose
    appearance: Appearance = Appearance()


class ScenePartKeyform(NamedTuple):
    """Part scene keyform with draw order."""

    keys: list[float]
    draw_order: float


SceneKeyform = SceneWarpKeyform | SceneRotationKeyform | ScenePartKeyform


class SceneBindingSnapshot(NamedTuple):
    """Versioned Part, Warp, or Rotation scene binding."""

    id: str
    axes: list[Axis]
    kind: str
    target_id: str
    keyforms: list[SceneKeyform]
    version: Version


class RotationData(NamedTuple):
    """Base angle and pose of a rotation transform."""

    base_angle: float
    pose: RotationPose


class WarpData(NamedTuple):
    """Warp grid dimensions, quad mode, and control points."""

    rows: int
    columns: int
    quad: bool
    points: list[Point]


class TransformSnapshot(NamedTuple):
    """Versioned rotation or warp transform and its parent relationships."""

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
    """Opacity and optional colors for an offscreen keyform."""

    opacity: float
    multiply: tuple[float, float, float] | None = None
    screen: tuple[float, float, float] | None = None


class OffscreenSpec(NamedTuple):
    """Complete input record for an offscreen composition layer."""

    id: str
    name: str
    part_id: str
    blend_mode: int = 0
    flags: int = 4
    masks: Sequence[str] = ()
    part_keyform_indices: Sequence[int] = ()
    keyforms: Sequence[OffscreenKeyform] = ()


class OffscreenSnapshot(NamedTuple):
    """Versioned offscreen layer with Part keyform indices."""

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


class GlueVertexPair(NamedTuple):
    """Paired vertex IDs and their individual glue weights."""

    vertex_a: int
    vertex_b: int
    weight_a: float
    weight_b: float


class GlueBinding(NamedTuple):
    """Parameter axes and sampled glue intensities."""

    axes: list[Axis]
    intensities: list[float]


class GlueSpec(NamedTuple):
    """Complete input record for glue between two meshes."""

    id: str
    name: str
    mesh_a_id: str
    mesh_b_id: str
    pairs: Sequence[GlueVertexPair]
    intensity: float = 0
    binding: GlueBinding | None = None


class GlueSnapshot(NamedTuple):
    """Versioned glue record, including its optional parameter binding."""

    id: str
    runtime_id: str
    name: str
    mesh_a_id: str
    mesh_b_id: str
    pairs: list[GlueVertexPair]
    intensity: float
    binding: GlueBinding | None
    version: Version


class BlendKeyTableSpec(NamedTuple):
    """BlendShape parameter keys and the base key index."""

    id: str
    parameter_id: str
    keys: Sequence[float]
    base_key_idx: int


class BlendKeyTableSnapshot(NamedTuple):
    """Versioned BlendShape key table."""

    id: str
    parameter_id: str
    keys: list[float]
    base_key_idx: int
    version: Version


class BlendConstraintSpec(NamedTuple):
    """BlendShape parameter keys and constraint weights."""

    id: str
    parameter_id: str
    keys: Sequence[float]
    weights: Sequence[float]


class BlendConstraintSnapshot(NamedTuple):
    """Versioned BlendShape constraint."""

    id: str
    parameter_id: str
    keys: list[float]
    weights: list[float]
    version: Version


class BlendMeshDelta(NamedTuple):
    """BlendShape mesh position and drawing deltas."""

    positions: Sequence[Point]
    opacity: float | None = None
    draw_order: float | None = None
    multiply: tuple[float, float, float] | None = None
    screen: tuple[float, float, float] | None = None


class BlendWarpDelta(NamedTuple):
    """BlendShape warp-control-point and appearance deltas."""

    points: Sequence[Point]
    opacity: float | None = None
    multiply: tuple[float, float, float] | None = None
    screen: tuple[float, float, float] | None = None


class BlendRotationDelta(NamedTuple):
    """BlendShape rotation-pose and appearance deltas."""

    origin: Point | None = None
    angle: float | None = None
    scale: float | None = None
    opacity: float | None = None
    multiply: tuple[float, float, float] | None = None
    screen: tuple[float, float, float] | None = None


class BlendPartDelta(NamedTuple):
    """BlendShape Part draw-order delta."""

    draw_order: float


class BlendGlueDelta(NamedTuple):
    """BlendShape glue-intensity delta."""

    intensity: float


class BlendOffscreenDelta(NamedTuple):
    """BlendShape offscreen opacity and color deltas."""

    opacity: float
    multiply: tuple[float, float, float] | None = None
    screen: tuple[float, float, float] | None = None


BlendDelta = (
    BlendMeshDelta | BlendWarpDelta | BlendRotationDelta |
    BlendPartDelta | BlendGlueDelta | BlendOffscreenDelta
)


class BlendBindingSpec(NamedTuple):
    """BlendShape target, key table, constraints, and target-specific deltas."""

    id: str
    target_id: str
    target_kind: str
    key_table_id: str
    constraint_ids: Sequence[str]
    keyforms: Sequence[BlendDelta]


class BlendBindingSnapshot(NamedTuple):
    """Versioned BlendShape binding."""

    id: str
    target_id: str
    target_kind: str
    key_table_id: str
    constraint_ids: list[str]
    keyforms: list[BlendDelta]
    version: Version


class ResourceIssue(NamedTuple):
    """Missing or invalid external asset reported by resource diagnosis."""

    asset_id: str
    code: str
    message: str


class StructureIssue(NamedTuple):
    """Persisted-document validation issue for an object."""

    object_id: str
    code: str
    message: str


class ParameterSample(NamedTuple):
    """Requested and sampled parameter values, plus clamp status."""

    id: str
    requested: float
    value: float
    clamped: bool


class DrawableSample(NamedTuple):
    """Evaluated runtime positions of one drawable."""

    id: str
    positions: list[Point]


class Evaluation(NamedTuple):
    """Compact CPU evaluation of parameters and drawable positions."""

    parameters: list[ParameterSample]
    drawables: list[DrawableSample]


class DrawableSnapshot(NamedTuple):
    """Full evaluated drawable geometry and render attributes."""

    id: str
    runtime_id: str
    part_id: str
    raw_blend_mode: int | None
    texture_asset_id: str
    texture_slot: int
    positions: list[Point]
    uvs: list[Point]
    indices: list[int]
    draw_order: int
    render_order: int
    opacity: float
    multiply_color: tuple[float, float, float, float]
    screen_color: tuple[float, float, float, float]
    blend_mode: str
    enabled: bool
    visible: bool
    double_sided: bool
    inverted_mask: bool
    masks: list[str]


class EvaluatedOffscreenSnapshot(NamedTuple):
    """Evaluated offscreen layer state and composition order."""

    id: str
    runtime_id: str
    owner_part_id: str
    parent_offscreen_id: str | None
    render_order: int
    opacity: float
    enabled: bool
    blend_mode: int
    flags: int
    masks: list[str]
    multiply_color: tuple[float, float, float, float]
    screen_color: tuple[float, float, float, float]


class EvaluationSnapshot(NamedTuple):
    """Full CPU evaluation with source version, drawables, and render plan."""

    version: Version
    source_revision: int
    canvas: CanvasSnapshot
    parameters: list[ParameterSample]
    drawables: list[DrawableSnapshot]
    offscreens: list[EvaluatedOffscreenSnapshot]
    render_plan: list[tuple[str, str]]


def _evaluation_snapshot(data) -> EvaluationSnapshot:
    version, revision, canvas, parameters, drawables, offscreens, plan = data
    return EvaluationSnapshot(
        version, revision, CanvasSnapshot(*canvas),
        [ParameterSample(*item) for item in parameters],
        [DrawableSnapshot(*item[0], *item[1]) for item in drawables],
        [EvaluatedOffscreenSnapshot(*item) for item in offscreens], plan,
    )


class SaveResult(NamedTuple):
    """Save receipt with the actual manifest path and publication status.

    ``durable`` reports whether directory synchronization succeeded. ``warnings``
    describe publication issues; ``history_warnings`` concern older resources
    referenced by undo history.
    """

    manifest: Path
    durable: bool
    warnings: list[str]
    history_warnings: list[str]


class ImportResult(NamedTuple):
    """Import version, MOC version, resource diagnostics, and warnings."""

    version: Version
    moc_version: int
    diagnostics: list[ResourceIssue]
    warnings: list[str]


class PsdImportResult(NamedTuple):
    """Published PSD project and imported artwork summary."""

    version: Version
    manifest: Path
    width: int
    height: int
    raster_layers: int
    groups: int
    durable: bool
    warnings: list[str]


class ExportResult(NamedTuple):
    """Export publication and durability status with warnings."""

    published: bool
    durable: bool
    warnings: list[str]


_sessions: WeakSet[Session] = WeakSet()


class Edit:
    """Candidate edit that publishes all commands together on successful exit.

    Any failed command aborts the candidate, even if its exception is caught.
    An exception leaving the ``with`` block cancels it.
    """

    def __init__(self, native) -> None:
        self._native = native
        self._closed = False

    def __enter__(self) -> Edit:
        """Return this edit for use in a context manager."""
        return self

    def parameter(self, parameter_id: str) -> ParameterSnapshot | None:
        """Read a parameter after preceding edit operations, or None if absent."""
        raw = self._native.parameter(parameter_id)
        return ParameterSnapshot(*raw) if raw is not None else None

    def mesh(self, mesh_id: str) -> MeshSnapshot | None:
        """Read a mesh after preceding edit operations, or None if absent."""
        raw = self._native.mesh(mesh_id)
        return MeshSnapshot(*raw) if raw is not None else None

    def __exit__(self, exception_type, exception, traceback) -> bool:
        """Commit on success, cancel on exception, and never suppress the exception."""
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
        """Read an absolute PNG path and add its size and content hash as an asset."""
        self._call(lambda: self._native.add_png_asset(asset_id, name, str(absolute_path)))

    def add_png_asset_from_base(
        self, asset_id: str, name: str, absolute_base: Path, relative_path: Path
    ) -> None:
        """Add a PNG resolved from a relative path and an explicit absolute base."""
        self._call(lambda: self._native.add_png_asset_from_base(
            asset_id, name, str(absolute_base), str(relative_path)
        ))

    def replace_png_asset(self, asset_id: str, name: str, absolute_path: Path) -> None:
        """Replace an asset with a PNG; new dimensions and content are allowed."""
        self._call(lambda: self._native.replace_png_asset(asset_id, name, str(absolute_path)))

    def relocate_png_asset(self, asset_id: str, absolute_path: Path) -> None:
        """Change an asset path only if the new PNG matches its dimensions and hash."""
        self._call(lambda: self._native.relocate_png_asset(asset_id, str(absolute_path)))

    def replace_canvas(
        self, width: float, height: float, origin: Point, pixels_per_unit: float
    ) -> None:
        """Replace the canvas size, pixel origin, and pixels-per-unit scale."""
        self._call(
            lambda: self._native.replace_canvas(
                width, height, origin[0], origin[1], pixels_per_unit
            )
        )

    def replace_draw_order_groups(self, groups: Sequence[DrawOrderGroup]) -> None:
        """Replace all explicit draw-order groups in this document."""
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
        """Create a Part with an optional organization parent."""
        self._call(
            lambda: self._native.create_part(
                part_id, name, parent_id, enabled, draw_order
            )
        )

    def replace_part(
        self, part_id: str, name: str, parent_id: str, enabled: bool, draw_order: float
    ) -> None:
        """Replace an existing Part while preserving its identity."""
        self._call(
            lambda: self._native.replace_part(
                part_id, name, parent_id, enabled, draw_order
            )
        )

    def create_rotation_transform(
        self, transform_id: str, name: str, rotation: RotationData,
        part_id: str | None = None, parent_id: str | None = None,
    ) -> None:
        """Create a rotation deformer, optionally under a Part or transform."""
        self._call(lambda: self._native.create_rotation_transform(
            transform_id, name, part_id, parent_id, _rotation_tuple(rotation)
        ))

    def create_warp_transform(
        self, transform_id: str, name: str, warp: WarpData,
        part_id: str | None = None, parent_id: str | None = None,
    ) -> None:
        """Create a warp deformer from grid dimensions and control points."""
        self._call(lambda: self._native.create_warp_transform(
            transform_id, name, part_id, parent_id, warp.rows, warp.columns,
            warp.quad, list(warp.points)
        ))

    def update_rotation(self, transform_id: str, rotation: RotationData) -> None:
        """Update the rotation data of an existing rotation transform."""
        self._call(lambda: self._native.update_rotation(transform_id, _rotation_tuple(rotation)))

    def replace_transform(self, transform: TransformSnapshot) -> None:
        """Replace a transform from a versioned snapshot, keeping its runtime identity."""
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
        """Create an offscreen composition layer from a complete specification."""
        self._call(lambda: self._native.create_offscreen(_offscreen_data(offscreen)))

    def replace_offscreen(self, offscreen: OffscreenSnapshot) -> None:
        """Replace an offscreen layer from a snapshot."""
        self._call(lambda: self._native.replace_offscreen(_offscreen_data(offscreen)))

    def replace_part_binding_with_offscreen(
        self, binding: SceneBindingSnapshot, offscreen: OffscreenSnapshot,
    ) -> None:
        """Replace a Part binding and its offscreen keyform mapping atomically."""
        if binding.kind != "part":
            self._native.abort()
            raise ValueError("Expected a Part scene binding")
        self._call(lambda: self._native.replace_part_binding_with_offscreen(
            binding.id, binding.target_id,
            [(axis.parameter_id, list(axis.keys)) for axis in binding.axes],
            [_scene_form_tuple("part", form) for form in binding.keyforms],
            _offscreen_data(offscreen),
        ))

    def create_glue(self, glue: GlueSpec) -> None:
        """Create glue between two meshes from paired vertex IDs and weights."""
        self._call(lambda: self._native.create_glue(_glue_data(glue)))

    def replace_glue(self, glue: GlueSnapshot) -> None:
        """Replace an existing glue record from a snapshot."""
        self._call(lambda: self._native.replace_glue(_glue_data(glue)))

    def create_mesh(self, mesh: MeshRecordSpec) -> None:
        """Create a mesh from a complete geometry and drawing record."""
        self._call(lambda: self._native.create_mesh(_mesh_record_data(mesh)))

    def replace_mesh(self, mesh: MeshRecordSnapshot) -> None:
        """Replace a mesh from a full snapshot while retaining its identity."""
        self._call(lambda: self._native.replace_mesh(_mesh_record_data(mesh)))

    def replace_topology(
        self,
        source: GeometrySnapshot,
        mesh: MeshRecordSnapshot,
        vertex_mapping: Mapping[int, int | None],
        binding: MeshBindingSnapshot | None = None,
        blend_bindings: Sequence[BlendBindingSnapshot] = (),
        glues: Sequence[GlueSnapshot] = (),
    ) -> None:
        """Replace topology and every affected binding/glue in one edit.

        The source geometry must be from the edit starting version. Map old vertex
        IDs to new IDs or None, and supply updated dependent objects as needed.
        """

        binding_data = None if binding is None else (
            binding.id, binding.mesh_id,
            [(axis.parameter_id, list(axis.keys)) for axis in binding.axes],
            [_mesh_form_tuple(form) for form in binding.keyforms],
        )
        self._call(lambda: self._native.replace_topology(
            (source.version, source.mesh_id, list(source.vertex_ids),
             list(source.positions), list(source.uvs), list(source.triangles),
             source.space, source.parent_id),
            _mesh_record_data(mesh), binding_data,
            [_blend_binding_data(item) for item in blend_bindings],
            [_glue_data(item) for item in glues],
            list(vertex_mapping.items()),
        ))

    def create_blend_key_table(self, table: BlendKeyTableSpec) -> None:
        """Create a BlendShape parameter key table."""
        self._call(lambda: self._native.create_blend_key_table(
            table.id, table.parameter_id, list(table.keys), table.base_key_idx,
        ))

    def replace_blend_key_table(self, table: BlendKeyTableSnapshot) -> None:
        """Replace an existing BlendShape key table."""
        self._call(lambda: self._native.replace_blend_key_table(
            table.id, table.parameter_id, list(table.keys), table.base_key_idx,
        ))

    def create_blend_constraint(self, constraint: BlendConstraintSpec) -> None:
        """Create a BlendShape constraint from keys and weights."""
        self._call(lambda: self._native.create_blend_constraint(
            constraint.id, constraint.parameter_id,
            list(constraint.keys), list(constraint.weights),
        ))

    def replace_blend_constraint(self, constraint: BlendConstraintSnapshot) -> None:
        """Replace an existing BlendShape constraint."""
        self._call(lambda: self._native.replace_blend_constraint(
            constraint.id, constraint.parameter_id,
            list(constraint.keys), list(constraint.weights),
        ))

    def create_blend_binding(self, binding: BlendBindingSpec) -> None:
        """Create a BlendShape binding with target-specific delta keyforms."""
        self._call(lambda: self._native.create_blend_binding(_blend_binding_data(binding)))

    def replace_blend_binding(self, binding: BlendBindingSnapshot) -> None:
        """Replace an existing BlendShape binding."""
        self._call(lambda: self._native.replace_blend_binding(_blend_binding_data(binding)))

    def update_warp_points(self, transform_id: str, points: Sequence[Point]) -> None:
        """Update the control points of an existing warp transform."""
        self._call(lambda: self._native.update_warp_points(transform_id, list(points)))

    def erase_object(self, object_id: str) -> None:
        """Delete an unreferenced object; referenced objects raise OBJECT_REFERENCED."""
        self._call(lambda: self._native.erase_object(object_id))

    def replace_parameter(
        self,
        parameter_id: str,
        name: str,
        minimum: float,
        maximum: float,
        default_value: float,
        repeat: bool = False,
        kind: str | None = None,
        runtime_id: str | None = None,
    ) -> None:
        """Replace a parameter; omitted kind and runtime ID keep their values."""
        self._call(
            lambda: self._native.replace_parameter(
                parameter_id, name, minimum, maximum, default_value, repeat, kind, runtime_id
            )
        )

    def set_organization_parent(self, part_id: str, parent_id: str) -> None:
        """Move a Part under another organization parent."""
        self._call(lambda: self._native.set_organization_parent(part_id, parent_id))

    def set_transform_parent(self, transform_id: str, parent_id: str | None) -> None:
        """Set or clear a transform deformer parent."""
        self._call(lambda: self._native.set_transform_parent(transform_id, parent_id))

    def set_transform_part(self, transform_id: str, part_id: str | None) -> None:
        """Set or clear the Part that owns a transform."""
        self._call(lambda: self._native.set_transform_part(transform_id, part_id))

    def set_deform_parent(self, mesh_id: str, transform_id: str) -> None:
        """Set the transform that deforms a mesh."""
        self._call(lambda: self._native.set_deform_parent(mesh_id, transform_id))

    def set_mesh_part(self, mesh_id: str, part_id: str) -> None:
        """Set the Part that owns a mesh."""
        self._call(lambda: self._native.set_mesh_part(mesh_id, part_id))

    def create_rectangle(
        self, mesh_id: str, name: str, asset_id: str, minimum: Point, maximum: Point
    ) -> None:
        """Create a four-vertex root mesh using an existing texture asset."""
        self._call(
            lambda: self._native.create_rectangle(mesh_id, name, asset_id, minimum, maximum)
        )

    def rename_mesh(self, mesh_id: str, name: str) -> None:
        """Change a mesh display name without replacing its geometry."""
        self._call(lambda: self._native.rename_mesh(mesh_id, name))

    def update_positions(
        self, mesh_id: str, vertex_ids: Sequence[int], positions: Sequence[Point]
    ) -> None:
        """Replace source positions for the supplied stable vertex IDs."""
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
        kind: str = "normal",
        runtime_id: str | None = None,
    ) -> None:
        """Create a parameter; runtime ID defaults to the internal UUID."""
        self._call(
            lambda: self._native.create_parameter(
                parameter_id, name, minimum, maximum, default_value, repeat, kind, runtime_id
            )
        )

    def create_mesh_binding(
        self,
        binding_id: str,
        mesh_id: str,
        axes: Sequence[Axis],
        forms: Sequence[MeshKeyform],
    ) -> None:
        """Create a complete Cartesian grid of mesh parameter keyforms."""
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
        """Replace a complete mesh parameter binding and its keyforms."""
        self._call(lambda: self._native.replace_mesh_binding(
            binding_id, mesh_id,
            [(axis.parameter_id, list(axis.keys)) for axis in axes],
            [_mesh_form_tuple(form) for form in forms],
        ))

    def set_mesh_keyform(self, binding_id: str, form: MeshKeyform) -> None:
        """Update an existing key combination in a mesh binding."""
        self._call(lambda: self._native.set_mesh_keyform(
            binding_id, _mesh_form_tuple(form)
        ))

    def update_mesh_properties(self, mesh_id: str, properties: MeshProperties) -> None:
        """Replace drawing fields while preserving mesh geometry and identity."""
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
        """Create a complete part, rotation, or warp parameter binding."""
        self._call(lambda: self._native.create_scene_binding(
            binding_id, kind, target_id,
            [(axis.parameter_id, list(axis.keys)) for axis in axes],
            [_scene_form_tuple(kind, form) for form in forms],
        ))

    def replace_scene_binding(
        self, binding_id: str | SceneBindingSnapshot, kind: str | None = None,
        target_id: str | None = None, axes: Sequence[Axis] | None = None,
        forms: Sequence[SceneKeyform] | None = None,
    ) -> None:
        """Replace a complete scene binding, optionally from a snapshot."""
        if isinstance(binding_id, SceneBindingSnapshot):
            if any(value is not None for value in (kind, target_id, axes, forms)):
                raise TypeError("snapshot cannot be combined with binding fields")
            snapshot = binding_id
            binding_id, kind, target_id = snapshot.id, snapshot.kind, snapshot.target_id
            axes, forms = snapshot.axes, snapshot.keyforms
        if kind is None or target_id is None or axes is None or forms is None:
            raise TypeError("replace_scene_binding requires a snapshot or all binding fields")
        self._call(lambda: self._native.replace_scene_binding(
            binding_id, kind, target_id,
            [(axis.parameter_id, list(axis.keys)) for axis in axes],
            [_scene_form_tuple(kind, form) for form in forms],
        ))

    def set_scene_keyform(self, binding_id: str, form: SceneKeyform) -> None:
        """Update an existing key combination in a scene binding."""
        self._call(lambda: self._native.set_scene_keyform(
            binding_id, _scene_kind(form), _scene_form_tuple(_scene_kind(form), form)
        ))

    def commit(self) -> Version:
        """Publish this edit atomically and return the new session version."""
        try:
            return self._native.commit()
        finally:
            self._closed = True

    def cancel(self) -> None:
        """Discard this edit without publishing its candidate changes."""
        self._native.cancel()
        self._closed = True


class Session:
    """Thread-safe authoring session for one current project.

    Mutations are made through ``edit``; reads return detached snapshots.
    ``document_id`` is a canonical UUID string, ``origin`` is a pixel ``(x, y)``
    tuple, and ``pixels_per_unit`` converts source pixels to runtime units.
    """

    def __init__(
        self,
        document_id: str,
        width: float,
        height: float,
        origin: Point,
        pixels_per_unit: float,
    ) -> None:
        """Create an empty document with a UUID and canvas in source pixels."""
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
        """Create an empty session with explicit undo step and byte limits."""
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
        """Start an atomic edit, optionally requiring the current version to match."""
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
        """Replace this session with an empty project and return its new version."""
        return self._native.new_project(
            document_id,
            width,
            height,
            origin[0],
            origin[1],
            pixels_per_unit,
            expected_version,
        )

    def save(
        self, absolute_path: Path, expected_version: Version | None = None,
        *, on_exists: str = "error",
    ) -> SaveResult:
        """Save to an absolute path and return a receipt with ``manifest``.

        ``on_exists='new'`` picks a numbered sibling when another project already
        occupies the destination. It never overwrites a project or bypasses a
        ``PROJECT_CONFLICT`` on the current session's own saved path. Inspect
        the result's ``durable`` flag and warnings after a successful save.
        """
        if on_exists not in ("error", "new"):
            raise ValueError("on_exists must be 'error' or 'new'")
        if not absolute_path.is_absolute():
            raise ValueError("Project path must be absolute")
        candidate = absolute_path
        for number in range(0, 1001):
            try:
                manifest, durable, warnings, history_warnings = self._native.save(
                    str(candidate), expected_version
                )
                return SaveResult(Path(manifest), durable, warnings, history_warnings)
            except SdkFailure as error:
                if on_exists != "new" or error.code != "DESTINATION_EXISTS" or number == 1000:
                    raise
                if absolute_path.name.endswith(".kasane.json"):
                    stem = absolute_path.name.removesuffix(".kasane.json")
                    candidate = absolute_path.with_name(f"{stem}-{number + 1}.kasane.json")
                else:
                    candidate = absolute_path.with_name(f"{absolute_path.name}-{number + 1}")
        raise RuntimeError("No free project destination found")

    def import_model3(
        self, absolute_path: Path, expected_version: Version | None = None
    ) -> ImportResult:
        """Replace this session from an absolute model3 JSON path.

        Return an ImportResult with the MOC version, resource diagnostics, and warnings.
        """

        version, moc_version, diagnostics, warnings = self._native.import_model3(
            str(absolute_path), expected_version
        )
        return ImportResult(
            version, moc_version, [ResourceIssue(*item) for item in diagnostics], warnings
        )

    def import_psd(
        self, absolute_path: Path, destination: Path,
        expected_version: Version | None = None,
    ) -> PsdImportResult:
        """Import a layered PSD into a new project directory and replace this session.

        Both paths must be absolute; the destination must not already exist.
        The session is preserved if decoding or publication fails.
        """
        version, manifest, width, height, layers, groups, durable, warnings = (
            self._native.import_psd(
                str(absolute_path), str(destination), expected_version
            )
        )
        return PsdImportResult(
            version, Path(manifest), width, height, layers, groups, durable, warnings
        )

    def import_bare_moc3(
        self,
        absolute_path: Path,
        texture_map: Mapping[int, Path],
        expected_version: Version | None = None,
    ) -> ImportResult:
        """Replace this session from a bare MOC3 and absolute texture-slot paths."""
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
        """Publish a MOC3/model3/texture package without changing document version."""
        return ExportResult(*self._native.export_package(str(absolute_path), expected_version))

    def undo(self) -> Version:
        """Undo one committed edit and return the resulting version."""
        return self._native.undo()

    def redo(self) -> Version:
        """Redo one committed edit and return the resulting version."""
        return self._native.redo()

    def mesh_ids(self) -> list[str]:
        """Return IDs of all committed meshes."""
        return self._native.mesh_ids()

    def asset_ids(self) -> list[str]:
        """Return IDs of all committed texture assets."""
        return self._native.asset_ids()

    def asset(self, asset_id: str) -> AssetSnapshot | None:
        """Return an asset snapshot, or None when the ID is absent."""
        raw = self._native.asset(asset_id)
        return AssetSnapshot(*raw) if raw is not None else None

    def references_to(self, object_id: str) -> list[str]:
        """Return IDs of committed objects referring to the given object."""
        return self._native.references_to(object_id)

    def parameter_ids(self) -> list[str]:
        """Return IDs of all committed parameters."""
        return self._native.parameter_ids()

    def binding_ids(self) -> list[str]:
        """Return IDs of all committed mesh bindings."""
        return self._native.binding_ids()

    def part_ids(self) -> list[str]:
        """Return IDs of all committed Parts."""
        return self._native.part_ids()

    def transform_ids(self) -> list[str]:
        """Return IDs of all committed rotation and warp transforms."""
        return self._native.transform_ids()

    def scene_binding_ids(self) -> list[str]:
        """Return IDs of all committed scene bindings."""
        return self._native.scene_binding_ids()

    def blend_key_table_ids(self) -> list[str]:
        """Return IDs of all committed BlendShape key tables."""
        return self._native.blend_key_table_ids()

    def blend_constraint_ids(self) -> list[str]:
        """Return IDs of all committed BlendShape constraints."""
        return self._native.blend_constraint_ids()

    def blend_binding_ids(self) -> list[str]:
        """Return IDs of all committed BlendShape bindings."""
        return self._native.blend_binding_ids()

    def glue_ids(self) -> list[str]:
        """Return IDs of all committed glue objects."""
        return self._native.glue_ids()

    def offscreen_ids(self) -> list[str]:
        """Return IDs of all committed offscreen layers."""
        return self._native.offscreen_ids()

    def parameter(self, parameter_id: str) -> ParameterSnapshot | None:
        """Return a parameter snapshot, or None when the ID is absent."""
        raw = self._native.parameter(parameter_id)
        if raw is None:
            return None
        return ParameterSnapshot(*raw)

    def parameter_id(self, name_or_id: str) -> str:
        """Resolve an ID or unique display name to a parameter ID.

        IDs take precedence if a display name happens to equal another ID.
        Unknown or ambiguous names raise ValueError; use IDs for duplicates.
        """
        ids = self.parameter_ids()
        if name_or_id in ids:
            return name_or_id
        matches = [parameter_id for parameter_id in ids
                   if (parameter := self.parameter(parameter_id)) is not None
                   and parameter.name == name_or_id]
        if len(matches) == 1:
            return matches[0]
        if matches:
            raise ValueError(f"Ambiguous parameter name {name_or_id!r}; use an ID")
        try:
            UUID(name_or_id)
        except ValueError:
            pass
        else:
            # Preserve the native structured error for an unknown UUID.
            return name_or_id
        raise ValueError(f"Unknown parameter {name_or_id!r}; use an ID or unique name")

    def _parameter_values(self, values: Mapping[str, float]) -> dict[str, float]:
        resolved: dict[str, float] = {}
        for name_or_id, value in values.items():
            parameter_id = self.parameter_id(name_or_id)
            if parameter_id in resolved:
                raise ValueError(f"Parameter {parameter_id!r} was supplied twice")
            resolved[parameter_id] = value
        return resolved

    def mesh(self, mesh_id: str) -> MeshSnapshot | None:
        """Return a compact mesh snapshot, or None when the ID is absent."""
        raw = self._native.mesh(mesh_id)
        if raw is None:
            return None
        return MeshSnapshot(*raw)

    def mesh_record(self, mesh_id: str) -> MeshRecordSnapshot | None:
        """Return full geometry, drawing, and relationship data for a mesh."""
        raw = self._native.mesh_record(mesh_id)
        if raw is None:
            return None
        data, runtime_id, version = raw
        id, name, texture_asset_id, geometry, relations, appearance, drawing = data
        return MeshRecordSnapshot(
            id, runtime_id, name, MeshGeometryData(*geometry),
            MeshDrawingData(texture_asset_id, Appearance(*appearance), *drawing),
            relations[0], relations[1], version,
        )

    def mesh_properties(self, mesh_id: str) -> MeshPropertiesSnapshot | None:
        """Return a mesh drawing-properties snapshot, or None if absent."""
        raw = self._native.mesh_properties(mesh_id)
        if raw is None:
            return None
        texture_asset_id, appearance, draw_order, blend_mode, enabled, double_sided, inverted_mask, masks, version = raw
        return MeshPropertiesSnapshot(
            texture_asset_id, Appearance(*appearance), draw_order, blend_mode,
            enabled, double_sided, inverted_mask, masks, version,
        )

    def binding(self, binding_id: str) -> MeshBindingSnapshot | None:
        """Return a mesh binding snapshot, or None when the ID is absent."""
        return _binding_snapshot(self._native.binding(binding_id))

    def binding_for_mesh(self, mesh_id: str) -> MeshBindingSnapshot | None:
        """Return the binding of a mesh, or None when it is unbound."""
        return _binding_snapshot(self._native.binding_for_mesh(mesh_id))

    def scene_binding(self, binding_id: str) -> SceneBindingSnapshot | None:
        """Return a scene binding snapshot, or None when the ID is absent."""
        return _scene_binding_snapshot(self._native.scene_binding(binding_id))

    def binding_for_scene(self, target_id: str) -> SceneBindingSnapshot | None:
        """Return a scene binding for the target, or None if unbound."""
        return _scene_binding_snapshot(self._native.binding_for_scene(target_id))

    def part(self, part_id: str) -> PartSnapshot | None:
        """Return a Part snapshot, or None when the ID is absent."""
        raw = self._native.part(part_id)
        return PartSnapshot(*raw) if raw is not None else None

    def transform(self, transform_id: str) -> TransformSnapshot | None:
        """Return a rotation or warp snapshot, or None when the ID is absent."""
        raw = self._native.transform(transform_id)
        if raw is None:
            return None
        id, runtime_id, name, part_id, parent_id, kind, rotation, warp, enabled, appearance, version = raw
        rotation_data = RotationData(rotation[0], RotationPose((rotation[1][0], rotation[1][1]), *rotation[1][2:])) if rotation is not None else None
        warp_data = WarpData(*warp) if warp is not None else None
        return TransformSnapshot(id, runtime_id, name, part_id, parent_id, kind, rotation_data, warp_data, enabled, Appearance(*appearance), version)

    def offscreen(self, offscreen_id: str) -> OffscreenSnapshot | None:
        """Return an offscreen snapshot, or None when the ID is absent."""
        raw = self._native.offscreen(offscreen_id)
        if raw is None:
            return None
        id, runtime_id, name, part_id, blend_mode, flags, masks, indices, forms, version = raw
        return OffscreenSnapshot(
            id, runtime_id, name, part_id, blend_mode, flags, masks, indices,
            [OffscreenKeyform(*form) for form in forms], version,
        )

    def glue(self, glue_id: str) -> GlueSnapshot | None:
        """Return a glue snapshot, or None when the ID is absent."""
        raw = self._native.glue(glue_id)
        if raw is None:
            return None
        id, runtime_id, name, mesh_a_id, mesh_b_id, pairs, intensity, binding, version = raw
        typed_binding = None if binding is None else GlueBinding(
            [Axis(parameter_id, keys) for parameter_id, keys in binding[0]], binding[1],
        )
        return GlueSnapshot(
            id, runtime_id, name, mesh_a_id, mesh_b_id,
            [GlueVertexPair(*pair) for pair in pairs], intensity, typed_binding, version,
        )

    def blend_key_table(self, table_id: str) -> BlendKeyTableSnapshot | None:
        """Return a BlendShape key table snapshot, or None if absent."""
        raw = self._native.blend_key_table(table_id)
        return BlendKeyTableSnapshot(*raw) if raw is not None else None

    def blend_constraint(self, constraint_id: str) -> BlendConstraintSnapshot | None:
        """Return a BlendShape constraint snapshot, or None if absent."""
        raw = self._native.blend_constraint(constraint_id)
        return BlendConstraintSnapshot(*raw) if raw is not None else None

    def blend_binding(self, binding_id: str) -> BlendBindingSnapshot | None:
        """Return a BlendShape binding snapshot, or None if absent."""
        raw = self._native.blend_binding(binding_id)
        if raw is None:
            return None
        id, target_id, kind, table_id, constraint_ids, forms, version = raw
        keyforms: list[BlendDelta] = []
        for positions, origin, angle, scale, opacity, draw_order, intensity, multiply, screen in forms:
            if kind == "mesh":
                keyforms.append(BlendMeshDelta(positions, opacity, draw_order, multiply, screen))
            elif kind == "warp":
                keyforms.append(BlendWarpDelta(positions, opacity, multiply, screen))
            elif kind == "rotation":
                keyforms.append(BlendRotationDelta(origin, angle, scale, opacity, multiply, screen))
            elif kind == "part":
                keyforms.append(BlendPartDelta(draw_order))
            elif kind == "glue":
                keyforms.append(BlendGlueDelta(intensity))
            elif kind == "offscreen":
                keyforms.append(BlendOffscreenDelta(opacity, multiply, screen))
            else:
                raise ValueError("Unknown BlendShape target kind")
        return BlendBindingSnapshot(id, target_id, kind, table_id, constraint_ids, keyforms, version)

    def handle(self, kind: str, object_id: str) -> ObjectHandle:
        """Get a handle for a committed object of the given kind and ID."""
        return self._native.handle(kind, object_id)

    def resolve_handle(self, handle: ObjectHandle) -> None:
        """Raise SdkFailure if a handle no longer refers to this live object."""
        self._native.resolve_handle(handle)

    def mesh_by_handle(self, handle: ObjectHandle) -> MeshSnapshot:
        """Read a mesh through a validated object handle."""
        return MeshSnapshot(*self._native.mesh_by_handle(handle))

    def find_meshes_by_name(self, name: str) -> list[MeshSnapshot]:
        """Return every mesh whose display name matches exactly."""
        return [MeshSnapshot(*mesh) for mesh in self._native.find_meshes_by_name(name)]

    def require_unique_mesh(self, name: str) -> MeshSnapshot:
        """Return the only mesh with this name; reject missing or duplicate names."""
        return MeshSnapshot(*self._native.require_unique_mesh(name))

    def geometry(self, mesh_id: str) -> GeometrySnapshot | None:
        """Return versioned source geometry, or None when the mesh is absent."""
        raw = self._native.geometry(mesh_id)
        return GeometrySnapshot(*raw) if raw is not None else None

    def validate_structure(self) -> list[StructureIssue]:
        """List persistent-document structure problems without reading asset files."""
        return [StructureIssue(*issue) for issue in self._native.validate_structure()]

    def history_state(self) -> HistoryState:
        """Return undo/redo counts, estimated bytes, and configured limits."""
        return HistoryState(*self._native.history_state())

    def history_lengths(self) -> tuple[int, int]:
        """Return the current (undo_steps, redo_steps) pair."""
        return self._native.history_lengths()

    def estimated_content_bytes(self) -> int:
        """Estimate persistent document content size, not process RSS."""
        return self._native.estimated_content_bytes()

    def drain_events(self) -> list[EditEvent]:
        """Consume edit and undo/redo events published since the last drain."""
        return [EditEvent(*event) for event in self._native.drain_events()]

    def evaluate(self, values: Mapping[str, float]) -> Evaluation:
        """Evaluate parameter values to sampled parameters and runtime positions.

        Values may use parameter IDs or unique display names. This does not update
        the session preview state.
        """

        parameters, drawables = self._native.evaluate(self._parameter_values(values))
        return Evaluation(
            [ParameterSample(*sample) for sample in parameters],
            [DrawableSample(*sample) for sample in drawables],
        )

    def evaluate_snapshot(self, values: Mapping[str, float]) -> EvaluationSnapshot:
        """Return full render attributes and source version without changing preview.

        This copies drawable geometry arrays; use ``evaluate`` for positions only.
        """
        return _evaluation_snapshot(self._native.evaluate_snapshot(self._parameter_values(values)))

    def diagnose_resources(self) -> list[ResourceIssue]:
        """Check referenced asset files and return missing or damaged resources."""
        return [ResourceIssue(*item) for item in self._native.diagnose_resources()]

    def diagnose_geometry(
        self,
        min_triangle_area: float = 0,
        canvas_bounds: tuple[Point, Point] | None = None,
    ) -> list[GeometryIssue]:
        """Return non-blocking triangle and canvas-boundary authoring hints."""
        return [
            GeometryIssue(*item)
            for item in self._native.diagnose_geometry(min_triangle_area, canvas_bounds)
        ]

    @property
    def preview_values(self) -> dict[str, float]:
        """Return a copy of the current preview parameter values."""
        return self._native.preview_values()

    @property
    def preview_revision(self) -> int:
        """Return the preview-input revision."""
        return self._native.preview_revision()

    @property
    def preview_evaluation_count(self) -> int:
        """Return how many preview evaluations this session has performed."""
        return self._native.preview_evaluation_count()

    def preview_frame(self) -> Evaluation:
        """Return compact evaluated positions for the current preview values."""
        parameters, drawables = self._native.preview_frame()
        return Evaluation(
            [ParameterSample(*sample) for sample in parameters],
            [DrawableSample(*sample) for sample in drawables],
        )

    def preview_snapshot(self) -> EvaluationSnapshot:
        """Return full evaluated render state for the current preview values."""
        return _evaluation_snapshot(self._native.preview_snapshot())

    def set_preview_values(self, values: Mapping[str, float]) -> bool:
        """Set all preview parameter values; return whether preview input changed."""
        return self._native.set_preview_values(self._parameter_values(values))

    def set_preview_parameter(self, parameter_id: str, value: float) -> bool:
        """Set one preview value by ID or unique name; return whether it changed."""
        return self._native.set_preview_parameter(self.parameter_id(parameter_id), value)

    def reset_preview_values(self) -> bool:
        """Clear preview overrides; return whether preview input changed."""
        return self._native.reset_preview_values()

    @property
    def version(self) -> Version:
        """Current (session_id, generation, revision) tuple."""
        return self._native.version()

    @property
    def document_id(self) -> str:
        """UUID of the current project document."""
        return self._native.document_id()

    @property
    def canvas(self) -> CanvasSnapshot:
        """Return a copy of the current canvas settings."""
        return CanvasSnapshot(*self._native.canvas())

    @property
    def uv_v_origin(self) -> str:
        """``top`` or ``bottom`` origin for mesh UV V relative to PNG rows."""
        return self._native.uv_v_origin()

    @property
    def draw_order_groups(self) -> list[DrawOrderGroup] | None:
        """Copy of explicit draw-order groups, or None when no groups exist."""
        raw = self._native.draw_order_groups()
        return [DrawOrderGroup(*group) for group in raw] if raw is not None else None

    @property
    def evaluation_revision(self) -> int:
        """Revision of content that last affected evaluation output."""
        return self._native.evaluation_revision()

    @property
    def modified(self) -> bool:
        """Whether document content differs from the saved baseline."""
        return self._native.modified()

    @property
    def project_path(self) -> Path | None:
        """Return the manifest path after save/open, or None if unsaved."""
        value = self._native.project_path()
        return Path(value) if value is not None else None


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


def _mesh_record_data(value: MeshRecordSpec | MeshRecordSnapshot):
    geometry = value.geometry
    drawing = value.drawing
    return (
        value.id, value.name, drawing.texture_asset_id,
        (list(geometry.vertex_ids), list(geometry.positions), list(geometry.uvs),
         list(geometry.triangles)),
        (value.part_id, value.deformer_id), tuple(drawing.appearance),
        (drawing.draw_order, drawing.blend_mode, drawing.enabled, drawing.double_sided,
         drawing.inverted_mask, list(drawing.masks), drawing.raw_blend_mode),
    )


def _glue_data(value: GlueSpec | GlueSnapshot):
    binding = None if value.binding is None else (
        [(axis.parameter_id, list(axis.keys)) for axis in value.binding.axes],
        list(value.binding.intensities),
    )
    return (
        value.id, value.name, value.mesh_a_id, value.mesh_b_id,
        [tuple(pair) for pair in value.pairs], value.intensity, binding,
    )


def _blend_form_tuple(kind: str, form: BlendDelta):
    if kind == "mesh" and isinstance(form, BlendMeshDelta):
        return (list(form.positions), None, None, None, form.opacity, form.draw_order,
                None, form.multiply, form.screen)
    if kind == "warp" and isinstance(form, BlendWarpDelta):
        return (list(form.points), None, None, None, form.opacity, None,
                None, form.multiply, form.screen)
    if kind == "rotation" and isinstance(form, BlendRotationDelta):
        return ([], form.origin, form.angle, form.scale, form.opacity, None,
                None, form.multiply, form.screen)
    if kind == "part" and isinstance(form, BlendPartDelta):
        return ([], None, None, None, None, form.draw_order, None, None, None)
    if kind == "glue" and isinstance(form, BlendGlueDelta):
        return ([], None, None, None, None, None, form.intensity, None, None)
    if kind == "offscreen" and isinstance(form, BlendOffscreenDelta):
        return ([], None, None, None, form.opacity, None, None, form.multiply, form.screen)
    raise TypeError("Blend delta does not match target kind")


def _blend_binding_data(value: BlendBindingSpec | BlendBindingSnapshot):
    return (
        value.id, value.target_id, value.target_kind, value.key_table_id,
        list(value.constraint_ids),
        [_blend_form_tuple(value.target_kind, form) for form in value.keyforms],
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
    "BlendConstraintSnapshot",
    "BlendConstraintSpec",
    "BlendKeyTableSnapshot",
    "BlendKeyTableSpec",
    "BlendBindingSnapshot",
    "BlendBindingSpec",
    "BlendMeshDelta",
    "BlendWarpDelta",
    "BlendRotationDelta",
    "BlendPartDelta",
    "BlendGlueDelta",
    "BlendOffscreenDelta",
    "Appearance",
    "AssetSnapshot",
    "CanvasSnapshot",
    "DrawableSample",
    "DrawableSnapshot",
    "DrawableBounds",
    "DrawOrderGroup",
    "Edit",
    "EditEvent",
    "Evaluation",
    "EvaluationSnapshot",
    "ExportResult",
    "GeometrySnapshot",
    "GeometryIssue",
    "GlueBinding",
    "GlueSnapshot",
    "GlueSpec",
    "GlueVertexPair",
    "HistoryState",
    "ImportResult",
    "PsdImportResult",
    "EvaluatedOffscreenSnapshot",
    "MeshSnapshot",
    "MeshGeometryData",
    "MeshDrawingData",
    "MeshRecordSpec",
    "MeshRecordSnapshot",
    "MeshProperties",
    "MeshPropertiesSnapshot",
    "MeshBindingSnapshot",
    "MeshKeyform",
    "ObjectHandle",
    "ObservationFailure",
    "ObservationRun",
    "ObservedFrame",
    "Observer",
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
    "TextureRevision",
    "WarpData",
    "capabilities",
    "open_project",
]
