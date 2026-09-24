"""Python context manager and typed authoring commands."""
from __future__ import annotations

from pathlib import Path
from typing import Mapping, Sequence
from ._types import (
    Axis,
    BlendBindingSnapshot,
    BlendBindingSpec,
    BlendConstraintSnapshot,
    BlendConstraintSpec,
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
