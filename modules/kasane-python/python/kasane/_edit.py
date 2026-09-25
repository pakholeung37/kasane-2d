"""Python context manager and typed authoring commands."""
from __future__ import annotations

from pathlib import Path
import json
from typing import Mapping, Sequence
from ._types import (
    Axis,
    BlendBindingSnapshot,
    BlendBindingSpec,
    BlendConstraintSnapshot,
    BlendConstraintSpec,
    CdiDiagnostic,
    ExpressionDiagnostic,
    MotionDiagnostic,
    PoseDiagnostic,
    PhysicsDiagnostic,
    BlendKeyTableSnapshot,
    BlendKeyTableSpec,
    DrawOrderGroup,
    GeometrySnapshot,
    GlueSnapshot,
    GlueSpec,
    MeshBindingSnapshot,
    MeshKeyform,
    MeshProperties,
    MeshRecordSnapshot,
    MeshRecordSpec,
    MeshSnapshot,
    OffscreenSnapshot,
    OffscreenSpec,
    ParameterSnapshot,
    Point,
    RotationData,
    SceneBindingSnapshot,
    SceneKeyform,
    TransformSnapshot,
    Version,
    WarpData,
)
from ._conversion import (
    _blend_binding_data,
    _glue_data,
    _mesh_form_tuple,
    _mesh_record_data,
    _offscreen_data,
    _rotation_tuple,
    _scene_form_tuple,
    _scene_kind,
)


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

    def set_parameter_display_name(self, parameter_id: str, name: str) -> None:
        """Change the CDI display name without changing the runtime ID."""
        self._call(lambda: self._native.set_parameter_display_name(parameter_id, name))

    def set_part_display_name(self, part_id: str, name: str) -> None:
        """Change a Part's CDI display name without changing its runtime ID."""
        self._call(lambda: self._native.set_part_display_name(part_id, name))

    def create_parameter_group(
        self, group_id: str, runtime_id: str, name: str, parent_id: str | None = None
    ) -> None:
        """Create a CDI group. ``group_id`` is a project UUID."""
        self._call(lambda: self._native.create_parameter_group(
            group_id, runtime_id, name, parent_id
        ))

    def replace_parameter_group(
        self, group_id: str, runtime_id: str, name: str, parent_id: str | None = None
    ) -> None:
        """Change a CDI group's runtime ID, name, or parent."""
        self._call(lambda: self._native.replace_parameter_group(
            group_id, runtime_id, name, parent_id
        ))

    def set_parameter_group(self, parameter_id: str, group_id: str | None) -> None:
        """Assign a parameter to a CDI group or to the root level."""
        self._call(lambda: self._native.set_parameter_group(parameter_id, group_id))

    def set_combined_parameters(self, set_id: str, parameter_ids: Sequence[str]) -> None:
        """Create or replace an ordered CDI combined-parameter set."""
        self._call(lambda: self._native.set_combined_parameters(set_id, list(parameter_ids)))

    def import_cdi3(self, text: str) -> list[CdiDiagnostic]:
        """Import CDI into this edit; unresolved model IDs return diagnostics."""
        try:
            return [CdiDiagnostic(*item) for item in self._native.import_cdi3(text)]
        except BaseException:
            self._native.abort()
            raise

    def create_expression(
        self,
        expression_id: str,
        name: str,
        entries: Sequence[tuple[str, float, str]],
        fade_in: float | None = None,
        fade_out: float | None = None,
    ) -> None:
        """Create an expression from project parameter UUID, value, blend triples.

        Blend is ``add``, ``multiply``, ``overwrite``, or ``default``.
        """
        self._call(lambda: self._native.create_expression(
            expression_id, name, list(entries), fade_in, fade_out
        ))

    def import_expression3(
        self, expression_id: str, name: str, text: str
    ) -> list[ExpressionDiagnostic]:
        """Import exp3 into this edit; unresolved parameter IDs return diagnostics."""
        try:
            return [ExpressionDiagnostic(*item) for item in
                    self._native.import_expression3(expression_id, name, text)]
        except BaseException:
            self._native.abort()
            raise

    def replace_expression(
        self,
        expression_id: str,
        name: str,
        entries: Sequence[tuple[str, float, str]],
        fade_in: float | None = None,
        fade_out: float | None = None,
    ) -> None:
        """Replace a known-field expression within the current transaction."""
        self._call(lambda: self._native.replace_expression(
            expression_id, name, list(entries), fade_in, fade_out
        ))

    def create_motion(self, motion_id: str, name: str, duration: float, fps: float,
                      looping: bool = False, restricted_beziers: bool = True,
                      fade_in: float | None = None, fade_out: float | None = None) -> None:
        """Create an empty motion clip; tracks and events can be added in this edit."""
        self._call(lambda: self._native.create_motion(
            motion_id, name, duration, fps, looping, restricted_beziers, fade_in, fade_out
        ))

    def import_motion3(self, motion_id: str, name: str, text: str) -> list[MotionDiagnostic]:
        """Import motion3; missing parameter or Part targets return diagnostics."""
        try:
            return [MotionDiagnostic(*item) for item in self._native.import_motion3(motion_id, name, text)]
        except BaseException:
            self._native.abort()
            raise

    def replace_motion(self, clip: Mapping[str, object]) -> None:
        """Replace a known-field clip using a detached motion snapshot."""
        self._call(lambda: self._native.replace_motion_json(json.dumps(dict(clip))))

    def set_motion_groups(self, groups: Sequence[Mapping[str, object]]) -> None:
        """Replace model3 motion registrations; one clip may occur in several groups."""
        self._call(lambda: self._native.set_motion_groups_json(json.dumps(list(groups))))

    def create_motion_track(self, motion_id: str, track: Mapping[str, object]) -> None:
        """Append a track with a stable UUID and typed segment list."""
        self._call(lambda: self._native.create_motion_track_json(motion_id, json.dumps(dict(track))))

    def replace_motion_track(self, motion_id: str, track: Mapping[str, object]) -> None:
        """Replace one track while preserving its UUID."""
        self._call(lambda: self._native.replace_motion_track_json(motion_id, json.dumps(dict(track))))

    def set_motion_segment(self, motion_id: str, track_id: str, index: int,
                           segment: Mapping[str, object]) -> None:
        self._call(lambda: self._native.set_motion_segment_json(motion_id, track_id, index, json.dumps(dict(segment))))

    def insert_motion_segment(self, motion_id: str, track_id: str, index: int,
                              segment: Mapping[str, object]) -> None:
        self._call(lambda: self._native.insert_motion_segment_json(motion_id, track_id, index, json.dumps(dict(segment))))

    def move_motion_key(self, motion_id: str, track_id: str, index: int, time: float, value: float) -> None:
        """Move the initial point (index 0) or a segment endpoint."""
        self._call(lambda: self._native.move_motion_key(motion_id, track_id, index, time, value))

    def set_motion_event(self, motion_id: str, event: Mapping[str, object]) -> None:
        self._call(lambda: self._native.set_motion_event_json(motion_id, json.dumps(dict(event))))

    def remove_motion_track(self, motion_id: str, track_id: str) -> None:
        self._call(lambda: self._native.remove_motion_track(motion_id, track_id))

    def remove_motion_event(self, motion_id: str, event_id: str) -> None:
        self._call(lambda: self._native.remove_motion_event(motion_id, event_id))

    def set_motion_timing(self, motion_id: str, duration: float, fps: float, looping: bool,
                          fade_in: float | None = None, fade_out: float | None = None) -> None:
        self._call(lambda: self._native.set_motion_timing(motion_id, duration, fps, looping, fade_in, fade_out))

    def create_pose(self, pose_id: str, groups: Sequence[Sequence[tuple[str, Sequence[str]]]],
                    fade_in: float | None = None) -> None:
        """Create a Pose asset from ordered Part UUID groups and linked Part UUIDs."""
        entries = [[{"part": {"kind": "resolved", "part_id": part_id},
                     "links": [{"kind": "resolved", "part_id": linked} for linked in links],
                     "extensions": {}} for part_id, links in group] for group in groups]
        self._call(lambda: self._native.set_pose_json(json.dumps({
            "id": pose_id, "file_type": "Live2D Pose", "fade_in": fade_in,
            "groups": entries, "extensions": {}, "opaque_source_ids": None,
            "opaque_source_content_hash": None,
        })))

    def replace_pose(self, pose: Mapping[str, object]) -> None:
        """Replace a known-field Pose asset from its detached snapshot."""
        self._call(lambda: self._native.set_pose_json(json.dumps(dict(pose))))

    def import_pose3(self, pose_id: str, text: str) -> list[PoseDiagnostic]:
        """Import pose3; missing Part runtime IDs return diagnostics."""
        try:
            return [PoseDiagnostic(*item) for item in self._native.import_pose3(pose_id, text)]
        except BaseException:
            self._native.abort()
            raise

    def create_physics(self, physics_id: str, physics3: Mapping[str, object],
                       parameter_bindings: Mapping[str, str]) -> None:
        """Store typed physics3 rigs with runtime-ID to parameter-UUID bindings."""
        self._call(lambda: self._native.set_physics_json(json.dumps({
            "id": physics_id, "data": dict(physics3),
            "parameter_bindings": dict(parameter_bindings),
            "opaque_source_ids": None, "opaque_source_content_hash": None,
        })))

    def replace_physics(self, physics: Mapping[str, object]) -> None:
        """Replace a known-field Physics asset from a detached snapshot."""
        self._call(lambda: self._native.set_physics_json(json.dumps(dict(physics))))

    def import_physics3(self, physics_id: str, text: str) -> list[PhysicsDiagnostic]:
        """Import physics3; missing parameter runtime IDs return diagnostics."""
        try:
            return [PhysicsDiagnostic(*item) for item in self._native.import_physics3(physics_id, text)]
        except BaseException:
            self._native.abort()
            raise

    def discard_missing_attachment(self, path: str) -> None:
        """Explicitly omit one missing model3 attachment from future package exports."""
        self._call(lambda: self._native.discard_missing_attachment(path))

    def set_model3_settings(self, *, groups: object | None = None,
                            layout: object | None = None,
                            hit_areas: object | None = None,
                            user_data: str | None = None) -> None:
        """Set typed model3 metadata; group parameters and hit-area meshes use UUID references."""
        self._call(lambda: self._native.set_model3_settings_json(json.dumps({
            "groups": groups, "layout": layout, "hit_areas": hit_areas,
            "user_data": user_data,
            "extensions": {}, "source_runtime_ids": None, "source_content": None,
        })))

    def replace_model3_settings(self, settings: Mapping[str, object]) -> None:
        """Replace the detached model3 metadata snapshot."""
        self._call(lambda: self._native.set_model3_settings_json(json.dumps(dict(settings))))

    def set_package_attachments(self, attachments: Mapping[str, bytes]) -> None:
        """Store Sound/UserData file bytes in the project for standalone export."""
        self._call(lambda: self._native.set_package_attachments(list(attachments.items())))

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
