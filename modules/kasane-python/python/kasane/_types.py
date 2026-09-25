"""Public immutable records and value aliases."""
from __future__ import annotations

from pathlib import Path
from typing import NamedTuple, Sequence


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

    def runtime_to_source(self, point: Point) -> Point:
        """Convert an evaluated drawable's world runtime point to canvas pixels.

        This also works when the drawable has a deformer parent. Do not pass
        parent-local ``geometry.positions`` directly.
        """
        return (self.origin_x + point[0] * self.pixels_per_unit,
                self.origin_y - point[1] * self.pixels_per_unit)

    def source_to_runtime(self, point: Point) -> Point:
        """Convert a canvas pixel point to world runtime coordinates.

        The result is not a parent-local mesh geometry position.
        """
        return ((point[0] - self.origin_x) / self.pixels_per_unit,
                (self.origin_y - point[1]) / self.pixels_per_unit)


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


class CdiDiagnostic(NamedTuple):
    """A CDI import finding that can be repaired before strict export."""

    code: str
    path: str
    message: str


class ExpressionDiagnostic(NamedTuple):
    """An expression import target that needs model repair."""

    code: str
    path: str
    message: str


class MotionDiagnostic(NamedTuple):
    """A motion3 import issue that needs repair before strict export."""

    code: str
    path: str
    message: str


class PoseDiagnostic(NamedTuple):
    """A pose3 import Part reference that needs repair."""

    code: str
    path: str
    message: str


class PhysicsDiagnostic(NamedTuple):
    """A physics3 parameter reference that needs repair."""

    code: str
    path: str
    message: str


class ExpressionSnapshot(NamedTuple):
    """Values from a detached Expression preview at one animation time."""

    time: float
    parameters: dict[str, float]
    active_expressions: list[str]


class MotionSnapshot(NamedTuple):
    """Detached Motion preview values and events at one animation time."""

    time: float
    parameters: dict[str, float]
    part_opacity_channels: dict[str, float]
    part_opacities: dict[str, float]
    model_opacity: float
    active_motions: list[str]
    active_expressions: list[str]
    fired_events: list[tuple[str, str, str]]
    coverage: list[str]


class SeekCacheStats(NamedTuple):
    """Retained checkpoint estimates and work performed by the last successful seek."""

    budget_bytes: int
    estimated_bytes: int
    checkpoints: int
    last_restored_time: float
    last_replayed_steps: int
