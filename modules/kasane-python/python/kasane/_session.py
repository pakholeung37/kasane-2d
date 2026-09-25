"""Authoring session facade and script-runner session registry."""
from __future__ import annotations

from pathlib import Path
import json
from typing import Mapping
from uuid import UUID
from weakref import WeakSet
from ._native import NativeSession, ObjectHandle, SdkFailure
from ._edit import Edit
from ._animation import ExpressionPreview, MotionPreview, PhysicsPreview
from ._mesh_edit import remesh_rectangle_grid as _remesh_rectangle_grid
from ._types import (
    Appearance,
    AssetSnapshot,
    Axis,
    BlendBindingSnapshot,
    BlendConstraintSnapshot,
    BlendDelta,
    BlendGlueDelta,
    BlendKeyTableSnapshot,
    BlendMeshDelta,
    BlendOffscreenDelta,
    BlendPartDelta,
    BlendRotationDelta,
    BlendWarpDelta,
    CanvasSnapshot,
    DrawOrderGroup,
    DrawableSample,
    EditEvent,
    Evaluation,
    EvaluationSnapshot,
    ExportResult,
    GeometryIssue,
    GeometrySnapshot,
    GlueBinding,
    GlueSnapshot,
    GlueVertexPair,
    HistoryState,
    ImportResult,
    MeshBindingSnapshot,
    MeshDrawingData,
    MeshGeometryData,
    MeshPropertiesSnapshot,
    MeshRecordSnapshot,
    MeshSnapshot,
    OffscreenKeyform,
    OffscreenSnapshot,
    ParameterSample,
    ParameterSnapshot,
    PartSnapshot,
    Point,
    PsdImportResult,
    ResourceIssue,
    RotationData,
    RotationPose,
    SaveResult,
    SceneBindingSnapshot,
    StructureIssue,
    TransformSnapshot,
    Version,
    WarpData,
)
from ._conversion import (
    _binding_snapshot,
    _evaluation_snapshot,
    _scene_binding_snapshot,
)

_sessions: WeakSet[Session] = WeakSet()


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

    def display_info(self) -> dict[str, object]:
        """Return a detached snapshot of CDI metadata and UUID references."""
        return json.loads(self._native.display_info_json())

    def export_cdi3(self) -> str:
        """Encode CDI using current display names and runtime IDs."""
        return self._native.export_cdi3()

    def expression_ids(self) -> list[str]:
        """Return expression UUIDs in model3 registration order."""
        return self._native.expression_ids()

    def expression(self, expression_id: str) -> dict[str, object] | None:
        """Return a detached expression asset snapshot, including unknown fields."""
        raw = self._native.expression_json(expression_id)
        return json.loads(raw) if raw is not None else None

    def export_expression3(self, expression_id: str) -> str:
        """Encode one expression using current parameter runtime IDs."""
        return self._native.export_expression3(expression_id)

    def expression_preview(self) -> ExpressionPreview:
        """Capture an independent CPU Expression preview for scheduling and seek."""
        return ExpressionPreview(self._native.expression_preview())

    def motion_ids(self) -> list[str]:
        """Return persistent Motion clip UUIDs."""
        return self._native.motion_ids()

    def motion_preview(self) -> MotionPreview:
        """Capture an independent CPU Motion preview for scheduling and seek."""
        return MotionPreview(self._native.motion_preview())

    def physics_preview(self) -> PhysicsPreview:
        """Create a detached Physics rig preview from the current document."""
        return PhysicsPreview(self._native.physics_preview())

    def motion(self, motion_id: str) -> dict[str, object] | None:
        """Return a detached Motion clip snapshot."""
        raw = self._native.motion_json(motion_id)
        return json.loads(raw) if raw is not None else None

    def motion_groups(self) -> list[dict[str, object]]:
        """Return model3 Motion registrations and per-entry overrides."""
        return json.loads(self._native.motion_groups_json())

    def export_motion3(self, motion_id: str) -> str:
        """Encode a clip with current runtime IDs, rejecting unresolved targets."""
        return self._native.export_motion3(motion_id)

    def pose(self) -> dict[str, object] | None:
        """Return the detached Pose asset, if present."""
        raw = self._native.pose_json()
        return json.loads(raw) if raw is not None else None

    def export_pose3(self) -> str | None:
        """Encode Pose using current Part runtime IDs."""
        return self._native.export_pose3()

    def physics(self) -> dict[str, object] | None:
        """Return the detached Physics asset, if present."""
        raw = self._native.physics_json()
        return json.loads(raw) if raw is not None else None

    def missing_attachments(self) -> list[str]:
        """Return imported references that must be repaired or explicitly discarded."""
        return self._native.missing_attachments()

    def model3_settings(self) -> dict[str, object]:
        """Return Groups, Layout, HitAreas, and unsupported extension fields."""
        return json.loads(self._native.model3_settings_json())

    def package_attachments(self) -> dict[str, bytes]:
        """Return detached managed Sound/UserData bytes keyed by package path."""
        return dict(self._native.package_attachments())

    def export_physics3(self) -> str | None:
        """Encode Physics using current parameter runtime IDs."""
        return self._native.export_physics3()

    def remesh_rectangle_grid(
        self, mesh_id: str, columns: int, rows: int,
        expected_version: Version | None = None,
    ) -> MeshRecordSnapshot:
        """Atomically subdivide a rectangle and migrate dependent keyforms."""
        return _remesh_rectangle_grid(self, mesh_id, columns, rows, expected_version)

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


def open_project(absolute_path: Path) -> Session:
    """Open a saved project from an absolute path."""
    return Session._from_native(NativeSession.open(str(absolute_path)))
