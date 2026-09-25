use super::*;

pub(super) fn apply_command(
    edit: &mut EditSession<'_>,
    command: Command,
) -> Result<(), kasane_sdk::SdkError> {
    match command {
        Command::AddPng(asset) => edit.create_asset(asset)?,
        Command::CreateRectangle(mesh) => edit.create_mesh(*mesh)?,
        Command::CreateMesh(mesh) => edit.create_mesh(*mesh)?,
        Command::ReplaceMesh(mut mesh) => {
            if let Some(previous) = edit.candidate_document().get_mesh(&mesh.id) {
                mesh.runtime_id = previous.runtime_id.clone();
            }
            edit.replace_mesh(*mesh)?
        }
        Command::ReplaceTopology(source, replacement) => {
            let mut replacement = *replacement;
            if let Some(previous) = edit.candidate_document().get_mesh(&replacement.mesh.id) {
                replacement.mesh.runtime_id = previous.runtime_id.clone();
            }
            for glue in &mut replacement.glues {
                if let Some(previous) = edit.candidate_document().get_glue(&glue.id) {
                    glue.runtime_id = previous.runtime_id.clone();
                }
            }
            edit.replace_topology(&source, replacement)?
        }
        Command::RenameMesh(id, name) => edit.rename_mesh(&id, name)?,
        Command::UpdatePositions(id, ids, positions) => {
            edit.update_positions(&id, &ids, &positions)?
        }
        Command::CreateParameter(parameter) => edit.create_parameter(parameter)?,
        Command::CreateMeshBinding(binding) => edit.create_binding(binding)?,
        Command::ReplaceMeshBinding(binding) => edit.replace_binding(binding)?,
        Command::SetMeshKeyform(id, form) => edit.set_mesh_keyform(&id, form)?,
        Command::UpdateMeshProperties(id, props) => edit.update_mesh_properties(&id, props)?,
        Command::ReplaceCanvas(canvas) => edit.replace_canvas(canvas)?,
        Command::EraseObject(id) => edit.erase_object(&id)?,
        Command::ReplaceParameter(mut parameter, kind) => {
            if let Some(previous) = edit.candidate_document().get_parameter(&parameter.id) {
                if parameter.runtime_id.is_empty() {
                    parameter.runtime_id = previous.runtime_id.clone();
                }
                parameter.decimal_places = previous.decimal_places;
                parameter.kind = previous.kind;
            }
            if let Some(kind) = kind {
                parameter.kind = kind;
            }
            edit.replace_parameter(parameter)?
        }
        Command::SetOrganizationParent(id, parent) => edit.set_organization_parent(&id, &parent)?,
        Command::SetTransformParent(id, parent) => {
            edit.set_transform_parent(&id, parent.map(Into::into))?
        }
        Command::SetTransformPart(id, part) => {
            edit.set_transform_part(&id, part.map(Into::into))?
        }
        Command::SetDeformParent(id, parent) => edit.set_deform_parent(&id, &parent)?,
        Command::SetMeshPart(id, part) => edit.set_mesh_part(&id, &part)?,
        Command::ReplaceDrawOrderGroups(groups) => edit.replace_draw_order_groups(groups)?,
        Command::CreatePart(part) => edit.create_part(part)?,
        Command::ReplacePart(mut part) => {
            if let Some(previous) = edit.candidate_document().get_part(&part.id) {
                part.runtime_id = previous.runtime_id.clone();
            }
            edit.replace_part(part)?
        }
        Command::CreateTransform(transform) => edit.create_transform(transform)?,
        Command::ReplaceTransform(mut transform) => {
            if let Some(previous) = edit.candidate_document().get_transform(&transform.id) {
                transform.runtime_id = previous.runtime_id.clone();
            }
            edit.replace_transform(transform)?
        }
        Command::CreateOffscreen(value) => edit.create_offscreen(value)?,
        Command::ReplaceOffscreen(mut value) => {
            if let Some(previous) = edit.candidate_document().get_offscreen(&value.id) {
                value.runtime_id = previous.runtime_id.clone();
            }
            edit.replace_offscreen(value)?
        }
        Command::ReplacePartBindingWithOffscreen(binding, mut value) => {
            if let Some(previous) = edit.candidate_document().get_offscreen(&value.id) {
                value.runtime_id = previous.runtime_id.clone();
            }
            edit.replace_part_binding_with_offscreen(binding, value)?
        }
        Command::CreateGlue(value) => edit.create_glue(value)?,
        Command::ReplaceGlue(mut value) => {
            if let Some(previous) = edit.candidate_document().get_glue(&value.id) {
                value.runtime_id = previous.runtime_id.clone();
            }
            edit.replace_glue(value)?
        }
        Command::CreateBlendKeyTable(value) => edit.create_blend_key_table(value)?,
        Command::ReplaceBlendKeyTable(value) => edit.replace_blend_key_table(value)?,
        Command::CreateBlendConstraint(value) => edit.create_blend_constraint(value)?,
        Command::ReplaceBlendConstraint(value) => edit.replace_blend_constraint(value)?,
        Command::CreateBlendBinding(value) => edit.create_blend_binding(value)?,
        Command::ReplaceBlendBinding(value) => edit.replace_blend_binding(value)?,
        Command::UpdateRotation(id, rotation) => edit.update_rotation(&id, rotation)?,
        Command::UpdateWarpPoints(id, points) => edit.update_warp_points(&id, points)?,
        Command::ReplaceAsset(asset) => edit.replace_asset(asset)?,
        Command::CreateSceneBinding(binding) => edit.create_scene_binding(binding)?,
        Command::ReplaceSceneBinding(binding) => edit.replace_scene_binding(binding)?,
        Command::SetSceneKeyform(id, form) => edit.set_scene_keyform(&id, form)?,
        Command::SetParameterDisplayName(id, name) => edit.set_parameter_display_name(&id, name)?,
        Command::SetPartDisplayName(id, name) => edit.set_part_display_name(&id, name)?,
        Command::CreateCdiGroup(group) => edit.create_parameter_group(group)?,
        Command::ReplaceCdiGroup(group) => edit.replace_parameter_group(group)?,
        Command::SetParameterGroup(id, group) => edit.set_parameter_group(&id, group.as_deref())?,
        Command::SetCombinedParameters(set) => edit.set_combined_parameters(set)?,
        Command::CreateExpression(expression) => edit.create_expression(expression)?,
        Command::ReplaceExpression(expression) => edit.replace_expression(expression)?,
        Command::CreateMotion(clip) => edit.create_motion(clip)?,
        Command::ReplaceMotion(clip) => edit.replace_motion(clip)?,
        Command::SetMotionGroups(groups) => edit.set_motion_groups(groups)?,
        Command::CreateMotionTrack(id, track) => edit.create_motion_track(&id, track)?,
        Command::ReplaceMotionTrack(id, track) => edit.replace_motion_track(&id, track)?,
        Command::SetMotionSegment(id, track_id, index, segment) => {
            edit.set_motion_segment(&id, &track_id, index, segment)?
        }
        Command::InsertMotionSegment(id, track_id, index, segment) => {
            edit.insert_motion_segment(&id, &track_id, index, segment)?
        }
        Command::MoveMotionKey(id, track_id, index, point) => {
            edit.move_motion_key(&id, &track_id, index, point)?
        }
        Command::SetMotionEvent(id, event) => edit.set_motion_event(&id, event)?,
        Command::RemoveMotionTrack(id, track_id) => edit.remove_motion_track(&id, &track_id)?,
        Command::RemoveMotionEvent(id, event_id) => edit.remove_motion_event(&id, &event_id)?,
        Command::SetMotionTiming(id, duration, fps, looping, fade_in, fade_out) => {
            edit.set_motion_timing(&id, duration, fps, looping, fade_in, fade_out)?
        }
        Command::SetPose(pose) => edit.set_pose(pose)?,
        Command::SetPhysics(physics) => edit.set_physics(physics)?,
        Command::SetModel3Settings(settings) => edit.set_model3_settings(settings)?,
        Command::SetPackageAttachments(attachments) => edit.set_package_attachments(attachments)?,
    }
    Ok(())
}
