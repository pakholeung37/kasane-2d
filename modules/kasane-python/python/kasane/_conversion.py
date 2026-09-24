"""Conversions between public records and native tuples."""
from __future__ import annotations

from ._types import (
    Appearance,
    Axis,
    BlendBindingSnapshot,
    BlendBindingSpec,
    BlendDelta,
    BlendGlueDelta,
    BlendMeshDelta,
    BlendOffscreenDelta,
    BlendPartDelta,
    BlendRotationDelta,
    BlendWarpDelta,
    CanvasSnapshot,
    DrawableSnapshot,
    EvaluatedOffscreenSnapshot,
    EvaluationSnapshot,
    GlueSnapshot,
    GlueSpec,
    MeshBindingSnapshot,
    MeshKeyform,
    MeshRecordSnapshot,
    MeshRecordSpec,
    OffscreenSnapshot,
    OffscreenSpec,
    ParameterSample,
    RotationData,
    RotationPose,
    SceneBindingSnapshot,
    SceneKeyform,
    ScenePartKeyform,
    SceneRotationKeyform,
    SceneWarpKeyform,
)


def _evaluation_snapshot(data) -> EvaluationSnapshot:
    version, revision, canvas, parameters, drawables, offscreens, plan = data
    return EvaluationSnapshot(
        version, revision, CanvasSnapshot(*canvas),
        [ParameterSample(*item) for item in parameters],
        [DrawableSnapshot(*item[0], *item[1]) for item in drawables],
        [EvaluatedOffscreenSnapshot(*item) for item in offscreens], plan,
    )


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
