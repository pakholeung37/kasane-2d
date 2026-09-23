//! Candidate-document edits and atomic publication.
use std::collections::HashSet;

use crate::session::{object_exists_in, AuthoringSession};
use crate::types::*;
use kasane_core::draw_order::DrawOrderGroup;
use kasane_core::{
    BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable, Canvas, ChangeKind, Document,
    EditResult, Glue, ImageAsset, Mesh, MeshBinding, MeshKeyform, Offscreen, Parameter, Part,
    PartId, RotationTransform, SceneBinding, SceneKeyform, Status, Transform, TransformData,
    TransformId, Vec2, VertexId,
};

pub struct EditSession<'a> {
    pub(crate) session: Option<&'a mut AuthoringSession>,
    pub(crate) base_keys: HashSet<ObjectKey>,
    pub(crate) candidate: Option<Document>,
    pub(crate) label: String,
    pub(crate) before: Version,
    pub(crate) aborted: bool,
    pub(crate) kind: ChangeKind,
    pub(crate) object_ids: Vec<String>,
    pub(crate) erased_keys: HashSet<ObjectKey>,
}

impl EditSession<'_> {
    /// Read the current candidate when composing multiple operations in one edit.
    pub fn candidate_document(&self) -> &Document {
        self.candidate.as_ref().expect("edit candidate exists")
    }

    pub fn base_version(&self) -> Version {
        self.before
    }

    fn document(&mut self) -> &mut Document {
        self.candidate.as_mut().expect("edit candidate exists")
    }
    fn record(
        &mut self,
        result: EditResult,
        operation: &'static str,
        id: &str,
    ) -> Result<(), SdkError> {
        if !result.status.is_ok() {
            self.aborted = true;
            let mut error = SdkError::from_status(result.status, operation, vec![id.into()]);
            error.referrers = result.referrers.into_boxed_slice();
            return Err(error);
        }
        self.kind = merge_kind(self.kind, result.changes.kind);
        for id in result.changes.object_ids {
            if !self.object_ids.contains(&id) {
                self.object_ids.push(id);
            }
        }
        Ok(())
    }
    fn ensure_active(&self, operation: &'static str) -> Result<(), SdkError> {
        if self.aborted {
            Err(SdkError::new(
                "EDIT_ABORTED",
                "An earlier edit method failed",
                operation,
            ))
        } else {
            Ok(())
        }
    }
    fn abort_with(&mut self, error: SdkError) -> SdkError {
        self.aborted = true;
        error
    }
    pub fn replace_canvas(&mut self, canvas: Canvas) -> Result<(), SdkError> {
        self.ensure_active("replace_canvas")?;
        let id = self.document().id().to_owned();
        let result = self.document().replace_canvas(canvas);
        self.record(result, "replace_canvas", &id)
    }
    pub fn replace_draw_order_groups(
        &mut self,
        groups: Vec<DrawOrderGroup>,
    ) -> Result<(), SdkError> {
        self.ensure_active("replace_draw_order_groups")?;
        let id = self.document().id().to_owned();
        let result = self.document().replace_draw_order_groups(groups);
        self.record(result, "replace_draw_order_groups", &id)
    }
    pub fn erase_object(&mut self, id: &str) -> Result<(), SdkError> {
        self.ensure_active("erase_object")?;
        let original = self
            .base_keys
            .iter()
            .cloned()
            .into_iter()
            .find(|key| key.id == id);
        let result = self.document().erase_object(id);
        self.record(result, "erase_object", id)?;
        if let Some(key) = original {
            self.erased_keys.insert(key);
        }
        Ok(())
    }
    pub fn create_asset(&mut self, asset: ImageAsset) -> Result<(), SdkError> {
        self.ensure_active("create_asset")?;
        let id = asset.id.clone();
        let result = self.document().add_asset(asset);
        self.record(result, "create_asset", &id)
    }
    pub fn replace_asset(&mut self, asset: ImageAsset) -> Result<(), SdkError> {
        self.ensure_active("replace_asset")?;
        let id = asset.id.clone();
        let result = self.document().replace_asset(asset);
        self.record(result, "replace_asset", &id)
    }
    pub fn create_parameter(&mut self, parameter: Parameter) -> Result<(), SdkError> {
        self.ensure_active("create_parameter")?;
        let id = parameter.id.clone();
        let result = self.document().create_parameter(parameter);
        self.record(result, "create_parameter", &id)
    }
    pub fn replace_parameter(&mut self, parameter: Parameter) -> Result<(), SdkError> {
        self.ensure_active("replace_parameter")?;
        let id = parameter.id.clone();
        let result = self.document().replace_parameter(parameter);
        self.record(result, "replace_parameter", &id)
    }
    pub fn create_part(&mut self, part: Part) -> Result<(), SdkError> {
        self.ensure_active("create_part")?;
        let id = part.id.clone();
        let result = self.document().create_part(part);
        self.record(result, "create_part", &id)
    }
    pub fn replace_part(&mut self, part: Part) -> Result<(), SdkError> {
        self.ensure_active("replace_part")?;
        let id = part.id.clone();
        let result = self.document().replace_part(part);
        self.record(result, "replace_part", &id)
    }
    /// Organization hierarchy is independent of the transform hierarchy.
    pub fn set_organization_parent(
        &mut self,
        part_id: &str,
        parent_id: &str,
    ) -> Result<(), SdkError> {
        self.ensure_active("set_organization_parent")?;
        let Some(mut part) = self.document().get_part(part_id).cloned() else {
            return Err(self.abort_with(SdkError::from_status(
                Status::error("MISSING_PART", "Part does not exist"),
                "set_organization_parent",
                vec![part_id.into()],
            )));
        };
        part.parent_id = parent_id.into();
        let result = self.document().replace_part(part);
        self.record(result, "set_organization_parent", part_id)
    }
    pub fn create_transform(&mut self, transform: Transform) -> Result<(), SdkError> {
        self.ensure_active("create_transform")?;
        let id = transform.id.clone();
        let result = self.document().create_transform(transform);
        self.record(result, "create_transform", &id)
    }
    pub fn replace_transform(&mut self, transform: Transform) -> Result<(), SdkError> {
        self.ensure_active("replace_transform")?;
        let id = transform.id.clone();
        let result = self.document().replace_transform(transform);
        self.record(result, "replace_transform", &id)
    }
    /// Changes the transform parent while retaining the stored local coordinates.
    pub fn set_transform_parent(
        &mut self,
        transform_id: &str,
        parent_id: Option<TransformId>,
    ) -> Result<(), SdkError> {
        self.ensure_active("set_transform_parent")?;
        let Some(mut transform) = self.document().get_transform(transform_id).cloned() else {
            return Err(self.abort_with(SdkError::from_status(
                Status::error("MISSING_TRANSFORM", "Transform does not exist"),
                "set_transform_parent",
                vec![transform_id.into()],
            )));
        };
        transform.parent_id = parent_id;
        let result = self.document().replace_transform(transform);
        self.record(result, "set_transform_parent", transform_id)
    }
    pub fn set_transform_part(
        &mut self,
        transform_id: &str,
        part_id: Option<PartId>,
    ) -> Result<(), SdkError> {
        self.ensure_active("set_transform_part")?;
        let Some(mut transform) = self.document().get_transform(transform_id).cloned() else {
            return Err(self.abort_with(SdkError::from_status(
                Status::error("MISSING_TRANSFORM", "Transform does not exist"),
                "set_transform_part",
                vec![transform_id.into()],
            )));
        };
        transform.part_id = part_id;
        let result = self.document().replace_transform(transform);
        self.record(result, "set_transform_part", transform_id)
    }
    pub fn update_rotation(
        &mut self,
        transform_id: &str,
        rotation: RotationTransform,
    ) -> Result<(), SdkError> {
        self.ensure_active("update_rotation")?;
        let Some(mut transform) = self.document().get_transform(transform_id).cloned() else {
            return Err(self.abort_with(SdkError::from_status(
                Status::error("MISSING_TRANSFORM", "Transform does not exist"),
                "update_rotation",
                vec![transform_id.into()],
            )));
        };
        if transform.rotation().is_none() {
            return Err(self.abort_with(SdkError::from_status(
                Status::error("INVALID_TRANSFORM_TYPE", "Transform is not a rotation"),
                "update_rotation",
                vec![transform_id.into()],
            )));
        }
        transform.data = TransformData::Rotation(rotation);
        let result = self.document().replace_transform(transform);
        self.record(result, "update_rotation", transform_id)
    }
    pub fn update_warp_points(
        &mut self,
        transform_id: &str,
        points: Vec<Vec2>,
    ) -> Result<(), SdkError> {
        self.ensure_active("update_warp_points")?;
        let Some(mut transform) = self.document().get_transform(transform_id).cloned() else {
            return Err(self.abort_with(SdkError::from_status(
                Status::error("MISSING_TRANSFORM", "Transform does not exist"),
                "update_warp_points",
                vec![transform_id.into()],
            )));
        };
        let Some(warp) = transform.warp_mut() else {
            return Err(self.abort_with(SdkError::from_status(
                Status::error("INVALID_TRANSFORM_TYPE", "Transform is not a warp"),
                "update_warp_points",
                vec![transform_id.into()],
            )));
        };
        warp.points = points;
        let result = self.document().replace_transform(transform);
        self.record(result, "update_warp_points", transform_id)
    }
    pub fn create_mesh(&mut self, mesh: Mesh) -> Result<(), SdkError> {
        self.ensure_active("create_mesh")?;
        let id = mesh.id.clone();
        let result = self.document().create_mesh(mesh);
        self.record(result, "create_mesh", &id)
    }
    pub fn replace_mesh(&mut self, mesh: Mesh) -> Result<(), SdkError> {
        self.ensure_active("replace_mesh")?;
        let id = mesh.id.clone();
        let result = self.document().replace_mesh(mesh);
        self.record(result, "replace_mesh", &id)
    }
    /// Changes the mesh's deform parent while retaining source positions as local coordinates.
    pub fn set_deform_parent(&mut self, mesh_id: &str, transform_id: &str) -> Result<(), SdkError> {
        self.ensure_active("set_deform_parent")?;
        let Some(mut mesh) = self.document().get_mesh(mesh_id).cloned() else {
            return Err(self.abort_with(SdkError::from_status(
                Status::error("MISSING_MESH", "Mesh does not exist"),
                "set_deform_parent",
                vec![mesh_id.into()],
            )));
        };
        mesh.deformer_id = transform_id.into();
        let result = self.document().replace_mesh(mesh);
        self.record(result, "set_deform_parent", mesh_id)
    }
    pub fn set_mesh_part(&mut self, mesh_id: &str, part_id: &str) -> Result<(), SdkError> {
        self.ensure_active("set_mesh_part")?;
        let Some(mut mesh) = self.document().get_mesh(mesh_id).cloned() else {
            return Err(self.abort_with(SdkError::from_status(
                Status::error("MISSING_MESH", "Mesh does not exist"),
                "set_mesh_part",
                vec![mesh_id.into()],
            )));
        };
        mesh.part_id = part_id.into();
        let result = self.document().replace_mesh(mesh);
        self.record(result, "set_mesh_part", mesh_id)
    }
    pub fn rename_mesh(&mut self, id: &str, name: impl Into<String>) -> Result<(), SdkError> {
        self.ensure_active("rename_mesh")?;
        let result = self.document().rename_mesh(id, name.into());
        self.record(result, "rename_mesh", id)
    }
    pub fn update_mesh_properties(
        &mut self,
        id: &str,
        properties: MeshProperties,
    ) -> Result<(), SdkError> {
        self.ensure_active("update_mesh_properties")?;
        let Some(mut mesh) = self.document().get_mesh(id).cloned() else {
            return Err(self.abort_with(SdkError::from_status(
                Status::error("MISSING_MESH", "Mesh does not exist"),
                "update_mesh_properties",
                vec![id.into()],
            )));
        };
        mesh.texture_asset_id = properties.texture_asset_id;
        mesh.appearance = properties.appearance;
        mesh.draw_order = properties.draw_order;
        mesh.blend_mode = properties.blend_mode;
        mesh.enabled = properties.enabled;
        mesh.double_sided = properties.double_sided;
        mesh.inverted_mask = properties.inverted_mask;
        mesh.masks = properties.masks;
        let result = self.document().replace_mesh(mesh);
        self.record(result, "update_mesh_properties", id)
    }
    pub fn replace_topology(
        &mut self,
        source: &GeometrySnapshot,
        replacement: TopologyReplacement,
    ) -> Result<(), SdkError> {
        self.ensure_active("replace_topology")?;
        if source.version != self.before || source.mesh_id != replacement.mesh.id {
            let mut error = SdkError::new(
                "STALE_TOPOLOGY",
                "Topology snapshot is not from this edit's starting version and mesh",
                "replace_topology",
            );
            error.object_ids = vec![source.mesh_id.clone(), replacement.mesh.id.clone()];
            error.expected_version = Some(Box::new(source.version));
            error.actual_version = Some(Box::new(self.before));
            return Err(self.abort_with(error));
        }
        let id = replacement.mesh.id.clone();
        let result = self.document().replace_mesh_topology(
            replacement.mesh,
            replacement.binding,
            replacement.blend_bindings,
            replacement.glues,
            replacement.vertex_mapping,
        );
        self.record(result, "replace_topology", &id)
    }
    pub fn create_binding(&mut self, binding: MeshBinding) -> Result<(), SdkError> {
        self.ensure_active("create_binding")?;
        let id = binding.id.clone();
        let result = self.document().create_binding(binding);
        self.record(result, "create_binding", &id)
    }
    pub fn replace_binding(&mut self, binding: MeshBinding) -> Result<(), SdkError> {
        self.ensure_active("replace_binding")?;
        let id = binding.id.clone();
        let result = self.document().replace_binding(binding);
        self.record(result, "replace_binding", &id)
    }
    pub fn set_mesh_keyform(
        &mut self,
        binding_id: &str,
        form: MeshKeyform,
    ) -> Result<(), SdkError> {
        self.ensure_active("set_mesh_keyform")?;
        let result = self.document().set_mesh_keyform(binding_id, form);
        self.record(result, "set_mesh_keyform", binding_id)
    }
    pub fn create_scene_binding(&mut self, binding: SceneBinding) -> Result<(), SdkError> {
        self.ensure_active("create_scene_binding")?;
        let id = binding.id.clone();
        let result = self.document().create_scene_binding(binding);
        self.record(result, "create_scene_binding", &id)
    }
    pub fn replace_scene_binding(&mut self, binding: SceneBinding) -> Result<(), SdkError> {
        self.ensure_active("replace_scene_binding")?;
        let id = binding.id.clone();
        let result = self.document().replace_scene_binding(binding);
        self.record(result, "replace_scene_binding", &id)
    }
    pub fn set_scene_keyform(
        &mut self,
        binding_id: &str,
        form: SceneKeyform,
    ) -> Result<(), SdkError> {
        self.ensure_active("set_scene_keyform")?;
        let result = self.document().set_scene_keyform(binding_id, form);
        self.record(result, "set_scene_keyform", binding_id)
    }
    pub fn create_blend_key_table(&mut self, table: BlendShapeKeyTable) -> Result<(), SdkError> {
        self.ensure_active("create_blend_key_table")?;
        let id = table.id.clone();
        let result = self.document().create_blend_key_table(table);
        self.record(result, "create_blend_key_table", &id)
    }
    pub fn replace_blend_key_table(&mut self, table: BlendShapeKeyTable) -> Result<(), SdkError> {
        self.ensure_active("replace_blend_key_table")?;
        let id = table.id.clone();
        let result = self.document().replace_blend_key_table(table);
        self.record(result, "replace_blend_key_table", &id)
    }
    pub fn create_blend_constraint(
        &mut self,
        constraint: BlendShapeConstraint,
    ) -> Result<(), SdkError> {
        self.ensure_active("create_blend_constraint")?;
        let id = constraint.id.clone();
        let result = self.document().create_blend_constraint(constraint);
        self.record(result, "create_blend_constraint", &id)
    }
    pub fn replace_blend_constraint(
        &mut self,
        constraint: BlendShapeConstraint,
    ) -> Result<(), SdkError> {
        self.ensure_active("replace_blend_constraint")?;
        let id = constraint.id.clone();
        let result = self.document().replace_blend_constraint(constraint);
        self.record(result, "replace_blend_constraint", &id)
    }
    pub fn create_blend_binding(&mut self, binding: BlendShapeBinding) -> Result<(), SdkError> {
        self.ensure_active("create_blend_binding")?;
        let id = binding.id.clone();
        let result = self.document().create_blend_binding(binding);
        self.record(result, "create_blend_binding", &id)
    }
    pub fn replace_blend_binding(&mut self, binding: BlendShapeBinding) -> Result<(), SdkError> {
        self.ensure_active("replace_blend_binding")?;
        let id = binding.id.clone();
        let result = self.document().replace_blend_binding(binding);
        self.record(result, "replace_blend_binding", &id)
    }
    pub fn create_glue(&mut self, glue: Glue) -> Result<(), SdkError> {
        self.ensure_active("create_glue")?;
        let id = glue.id.clone();
        let result = self.document().create_glue(glue);
        self.record(result, "create_glue", &id)
    }
    pub fn replace_glue(&mut self, glue: Glue) -> Result<(), SdkError> {
        self.ensure_active("replace_glue")?;
        let id = glue.id.clone();
        let result = self.document().replace_glue(glue);
        self.record(result, "replace_glue", &id)
    }
    pub fn create_offscreen(&mut self, offscreen: Offscreen) -> Result<(), SdkError> {
        self.ensure_active("create_offscreen")?;
        let id = offscreen.id.clone();
        let result = self.document().create_offscreen(offscreen);
        self.record(result, "create_offscreen", &id)
    }
    pub fn replace_offscreen(&mut self, offscreen: Offscreen) -> Result<(), SdkError> {
        self.ensure_active("replace_offscreen")?;
        let id = offscreen.id.clone();
        let result = self.document().replace_offscreen(offscreen);
        self.record(result, "replace_offscreen", &id)
    }
    pub fn replace_part_binding_with_offscreen(
        &mut self,
        binding: SceneBinding,
        offscreen: Offscreen,
    ) -> Result<(), SdkError> {
        self.ensure_active("replace_part_binding_with_offscreen")?;
        let id = binding.id.clone();
        let result = self
            .document()
            .replace_part_binding_with_offscreen(binding, offscreen);
        self.record(result, "replace_part_binding_with_offscreen", &id)
    }
    pub fn update_positions(
        &mut self,
        mesh_id: &str,
        vertex_ids: &[VertexId],
        positions: &[Vec2],
    ) -> Result<(), SdkError> {
        self.ensure_active("update_positions")?;
        let result = self
            .document()
            .set_vertex_positions(mesh_id, vertex_ids, positions);
        self.record(result, "update_positions", mesh_id)
    }
    pub fn commit(mut self) -> Result<EditReceipt, SdkError> {
        let session = self.session.take().expect("attached edit has a session");
        self.commit_to(session)
    }

    /// Publish an owned edit workspace to its original session.
    pub fn commit_to(mut self, session: &mut AuthoringSession) -> Result<EditReceipt, SdkError> {
        self.ensure_active("commit")?;
        if session.version() != self.before {
            let mut error = SdkError::new("STALE_VERSION", "Document version changed", "commit");
            error.expected_version = Some(Box::new(self.before));
            error.actual_version = Some(Box::new(session.version()));
            return Err(error);
        }
        let candidate = self.candidate.take().expect("edit candidate exists");
        if let Some(issue) = candidate.validate_structure().into_iter().next() {
            return Err(SdkError::from_status(
                issue.status,
                "commit",
                vec![issue.object_id],
            ));
        }
        let identity_keys: HashSet<_> = self
            .erased_keys
            .iter()
            .filter(|key| object_exists_in(&candidate, key.kind, &key.id))
            .cloned()
            .collect();
        let identity_changed = !identity_keys.is_empty();
        let changed = identity_changed || !session.project.document().same_content(&candidate);
        let before_keys = changed.then(|| session.object_keys());
        let previous = changed.then(|| session.project.document().checkpoint());
        let mut evict_count = 0usize;
        if let Some(checkpoint) = &previous {
            let limits = session.history_limits;
            let candidate_bytes = candidate.estimated_content_bytes();
            let entry_bytes = checkpoint.estimated_bytes()
                + self.label.capacity()
                + std::mem::size_of::<String>()
                + identity_keys.capacity() * std::mem::size_of::<ObjectKey>()
                + identity_keys
                    .iter()
                    .map(|key| key.id.capacity())
                    .sum::<usize>()
                + session.project.root().as_os_str().len();
            let redo_bytes: usize = session.redo.iter().map(HistoryEntry::estimated_bytes).sum();
            let mut projected_bytes = candidate_bytes
                .saturating_add(entry_bytes)
                .saturating_add(redo_bytes)
                .saturating_add(
                    session
                        .done
                        .iter()
                        .map(HistoryEntry::estimated_bytes)
                        .sum::<usize>(),
                );
            let mut projected_steps = session.done.len() + 1;
            while (projected_bytes > limits.max_bytes || projected_steps > limits.max_steps)
                && evict_count < session.done.len()
            {
                projected_bytes =
                    projected_bytes.saturating_sub(session.done[evict_count].estimated_bytes());
                projected_steps -= 1;
                evict_count += 1;
            }
            if projected_bytes > limits.max_bytes || projected_steps > limits.max_steps {
                return Err(SdkError::new(
                    "HISTORY_LIMIT_EXCEEDED",
                    "Edit cannot fit the history budget",
                    "commit",
                ));
            }
        }
        session
            .project
            .publish_authoring_candidate(candidate, self.kind, identity_changed)
            .map_err(|s| SdkError::from_status(s, "commit", self.object_ids.clone()))?;
        if changed {
            session
                .preview
                .retain_parameters(session.project.document());
        }
        if let Some(keys) = &before_keys {
            session.refresh_incarnations(keys, &identity_keys);
        }
        if let Some(checkpoint) = previous {
            for _ in 0..evict_count {
                session.done.pop_front();
            }
            session.done.push_back(HistoryEntry {
                label: self.label.clone(),
                checkpoint,
                identity_keys,
                root: session.project.root(),
            });
            session.redo.clear();
        }
        let receipt = EditReceipt {
            label: self.label.clone(),
            before: self.before,
            after: session.version(),
            kind: if changed { self.kind } else { ChangeKind::None },
            object_ids: if changed {
                std::mem::take(&mut self.object_ids)
            } else {
                Vec::new()
            },
            changed,
        };
        if changed {
            session.events.push(receipt.clone());
        }
        Ok(receipt)
    }
}

fn merge_kind(a: ChangeKind, b: ChangeKind) -> ChangeKind {
    use ChangeKind::*;
    match (a, b) {
        (Structure, _) | (_, Structure) => Structure,
        (Resources, _) | (_, Resources) => Resources,
        (Positions, _) | (_, Positions) => Positions,
        (Metadata, _) | (_, Metadata) => Metadata,
        _ => None,
    }
}
