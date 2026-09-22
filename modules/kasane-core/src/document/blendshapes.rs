use super::*;
use crate::types::{
    BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable, BlendShapeTargetKind, ChangeKind,
    DeltaKeyforms, EditResult, ParameterKind, Status, TransformKind,
};

impl Document {
    pub fn blend_key_table_order(&self) -> &[String] {
        &self.blend_key_table_order
    }

    pub fn get_blend_key_table(&self, id: &str) -> Option<&BlendShapeKeyTable> {
        self.blend_key_tables.get(id)
    }

    pub(super) fn validate_blend_key_table(&self, table: &BlendShapeKeyTable) -> Status {
        if !valid_uuid(&table.id) {
            return Status::error("INVALID_ID", &table.id);
        }
        let param = match self.get_parameter(&table.parameter_id) {
            Some(p) => p,
            None => return Status::error("MISSING_PARAMETER", &table.parameter_id),
        };
        if param.kind != ParameterKind::BlendShape {
            return Status::error(
                "INVALID_PARAMETER_KIND",
                format!("{}: expected blend_shape parameter", table.parameter_id),
            );
        }
        if table.keys.is_empty() {
            return Status::error("INVALID_KEYS", "Key table must contain at least one key");
        }
        if table.base_key_idx >= table.keys.len() {
            return Status::error("INVALID_BASE_KEY", "Base key index out of bounds");
        }
        for (i, &k) in table.keys.iter().enumerate() {
            if !k.is_finite()
                || k < param.minimum
                || k > param.maximum
                || (i > 0 && k <= table.keys[i - 1])
            {
                return Status::error(
                    "INVALID_KEYS",
                    "Keys must be finite, strictly increasing, and within parameter range",
                );
            }
        }
        Status::ok()
    }

    pub fn create_blend_key_table(&mut self, table: BlendShapeKeyTable) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &table.id));
        }
        if self.contains_id(&table.id) {
            return self.failed(Status::error("DUPLICATE_ID", &table.id));
        }
        let s = self.validate_blend_key_table(&table);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = table.id.clone();
        let param_id = table.parameter_id.clone();
        self.blend_key_tables.insert(id.clone(), table);
        self.blend_key_table_order.push(id.clone());
        let meshes = self.mesh_order.clone();
        self.changed(ChangeKind::Structure, meshes, vec![id, param_id])
    }

    pub fn replace_blend_key_table(&mut self, table: BlendShapeKeyTable) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &table.id));
        }
        if !self.blend_key_tables.contains_key(&table.id) {
            return self.failed(Status::error("MISSING_KEY_TABLE", &table.id));
        }
        let s = self.validate_blend_key_table(&table);
        if !s.is_ok() {
            return self.failed(s);
        }
        for b in self.blend_bindings.values() {
            if b.key_table_id == table.id && b.keyforms.len() != table.keys.len() {
                return self.failed(Status::error(
                    "KEYFORMS_REQUIRED",
                    format!(
                        "{}: binding keyform count does not match new key table",
                        b.id
                    ),
                ));
            }
        }
        let id = table.id.clone();
        let param_id = table.parameter_id.clone();
        self.blend_key_tables.insert(id.clone(), table);
        let meshes = self.mesh_order.clone();
        self.changed(ChangeKind::Structure, meshes, vec![id, param_id])
    }

    pub fn blend_constraint_order(&self) -> &[String] {
        &self.blend_constraint_order
    }

    pub fn get_blend_constraint(&self, id: &str) -> Option<&BlendShapeConstraint> {
        self.blend_constraints.get(id)
    }

    pub(super) fn validate_blend_constraint(&self, constraint: &BlendShapeConstraint) -> Status {
        if !valid_uuid(&constraint.id) {
            return Status::error("INVALID_ID", &constraint.id);
        }
        let param = match self.get_parameter(&constraint.parameter_id) {
            Some(p) => p,
            None => return Status::error("MISSING_PARAMETER", &constraint.parameter_id),
        };
        if constraint.keys.len() != constraint.weights.len() || constraint.keys.is_empty() {
            return Status::error(
                "INVALID_LENGTH",
                "Constraint keys and weights must have matching non-zero lengths",
            );
        }
        for (i, (&k, &w)) in constraint
            .keys
            .iter()
            .zip(constraint.weights.iter())
            .enumerate()
        {
            if !k.is_finite()
                || k < param.minimum
                || k > param.maximum
                || (i > 0 && k <= constraint.keys[i - 1])
            {
                return Status::error(
                    "INVALID_KEYS",
                    "Constraint keys must be finite, strictly increasing, and within parameter range",
                );
            }
            if !w.is_finite() || w < 0.0 {
                return Status::error(
                    "INVALID_WEIGHT",
                    "Constraint weights must be finite and non-negative",
                );
            }
        }
        Status::ok()
    }

    pub fn create_blend_constraint(&mut self, constraint: BlendShapeConstraint) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &constraint.id));
        }
        if self.contains_id(&constraint.id) {
            return self.failed(Status::error("DUPLICATE_ID", &constraint.id));
        }
        let s = self.validate_blend_constraint(&constraint);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = constraint.id.clone();
        let param_id = constraint.parameter_id.clone();
        self.blend_constraints.insert(id.clone(), constraint);
        self.blend_constraint_order.push(id.clone());
        let meshes = self.mesh_order.clone();
        self.changed(ChangeKind::Structure, meshes, vec![id, param_id])
    }

    pub fn replace_blend_constraint(&mut self, constraint: BlendShapeConstraint) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &constraint.id));
        }
        if !self.blend_constraints.contains_key(&constraint.id) {
            return self.failed(Status::error("MISSING_CONSTRAINT", &constraint.id));
        }
        let s = self.validate_blend_constraint(&constraint);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = constraint.id.clone();
        let param_id = constraint.parameter_id.clone();
        self.blend_constraints.insert(id.clone(), constraint);
        let meshes = self.mesh_order.clone();
        self.changed(ChangeKind::Structure, meshes, vec![id, param_id])
    }

    pub fn blend_binding_order(&self) -> &[String] {
        &self.blend_binding_order
    }

    pub fn get_blend_binding(&self, id: &str) -> Option<&BlendShapeBinding> {
        self.blend_bindings.get(id)
    }

    pub fn blend_bindings_for_target(&self, target_id: &str) -> Vec<&BlendShapeBinding> {
        self.lookup()
            .blend_bindings
            .get(target_id)
            .into_iter()
            .flatten()
            .map(|id| &self.blend_bindings[id])
            .collect()
    }

    pub(super) fn validate_blend_binding(&self, b: &BlendShapeBinding) -> Status {
        if !valid_uuid(&b.id) {
            return Status::error("INVALID_ID", &b.id);
        }
        let key_table = match self.get_blend_key_table(&b.key_table_id) {
            Some(kt) => kt,
            None => return Status::error("MISSING_KEY_TABLE", &b.key_table_id),
        };
        for c_id in &b.constraint_ids {
            if !self.blend_constraints.contains_key(c_id) {
                return Status::error("MISSING_CONSTRAINT", c_id);
            }
        }
        if b.keyforms.len() != key_table.keys.len() {
            return Status::error(
                "INCOMPLETE_KEYFORMS",
                format!(
                    "{}: keyforms length {} does not match key table {}",
                    b.id,
                    b.keyforms.len(),
                    key_table.keys.len()
                ),
            );
        }
        match (&b.target_kind, &b.keyforms) {
            (BlendShapeTargetKind::Mesh, DeltaKeyforms::Mesh(forms)) => {
                let mesh = match self.get_mesh(&b.target_id) {
                    Some(m) => m,
                    None => return Status::error("MISSING_MESH", &b.target_id),
                };
                for f in forms {
                    if f.positions.len() != mesh.vertex_ids.len() {
                        return Status::error(
                            "INVALID_LENGTH",
                            format!(
                                "{}: delta positions len {} != mesh vertex len {}",
                                b.id,
                                f.positions.len(),
                                mesh.vertex_ids.len()
                            ),
                        );
                    }
                    for p in &f.positions {
                        if !p.x.is_finite() || !p.y.is_finite() {
                            return Status::error(
                                "INVALID_POSITION",
                                format!("{}: non-finite delta position", b.id),
                            );
                        }
                    }
                    if let Some(op) = f.opacity {
                        if !op.is_finite() {
                            return Status::error("INVALID_OPACITY", &b.id);
                        }
                    }
                    if let Some(order) = f.draw_order {
                        let s = validate_draw_order(order, &b.id);
                        if !s.is_ok() {
                            return s;
                        }
                    }
                    if let Some(mult) = f.multiply {
                        for &c in &mult {
                            if !c.is_finite() {
                                return Status::error("INVALID_COLOR", &b.id);
                            }
                        }
                    }
                    if let Some(scr) = f.screen {
                        for &c in &scr {
                            if !c.is_finite() {
                                return Status::error("INVALID_COLOR", &b.id);
                            }
                        }
                    }
                }
            }
            (BlendShapeTargetKind::Warp, DeltaKeyforms::Warp(forms)) => {
                let warp = match self.get_transform(&b.target_id) {
                    Some(w) if w.kind() == TransformKind::Warp => w,
                    _ => {
                        return Status::error(
                            "MISSING_OBJECT",
                            format!("{}: expected Warp transform", b.target_id),
                        )
                    }
                };
                for f in forms {
                    if f.points.len() != warp.warp().unwrap().points.len() {
                        return Status::error(
                            "INVALID_LENGTH",
                            format!(
                                "{}: delta points len {} != warp points len {}",
                                b.id,
                                f.points.len(),
                                warp.warp().unwrap().points.len()
                            ),
                        );
                    }
                    for p in &f.points {
                        if !p.x.is_finite() || !p.y.is_finite() {
                            return Status::error(
                                "INVALID_POSITION",
                                format!("{}: non-finite delta point", b.id),
                            );
                        }
                    }
                    if let Some(op) = f.opacity {
                        if !op.is_finite() {
                            return Status::error("INVALID_OPACITY", &b.id);
                        }
                    }
                    if let Some(mult) = f.multiply {
                        for &c in &mult {
                            if !c.is_finite() {
                                return Status::error("INVALID_COLOR", &b.id);
                            }
                        }
                    }
                    if let Some(scr) = f.screen {
                        for &c in &scr {
                            if !c.is_finite() {
                                return Status::error("INVALID_COLOR", &b.id);
                            }
                        }
                    }
                }
            }
            (BlendShapeTargetKind::Rotation, DeltaKeyforms::Rotation(forms)) => {
                match self.get_transform(&b.target_id) {
                    Some(r) if r.kind() == TransformKind::Rotation => r,
                    _ => {
                        return Status::error(
                            "MISSING_OBJECT",
                            format!("{}: expected Rotation transform", b.target_id),
                        )
                    }
                };
                for f in forms {
                    if let Some(orig) = f.origin {
                        if !orig.x.is_finite() || !orig.y.is_finite() {
                            return Status::error("INVALID_POSITION", &b.id);
                        }
                    }
                    if let Some(angle) = f.angle {
                        if !angle.is_finite() {
                            return Status::error("INVALID_ROTATION", &b.id);
                        }
                    }
                    if let Some(scale) = f.scale {
                        if !scale.is_finite() {
                            return Status::error("INVALID_ROTATION", &b.id);
                        }
                    }
                    if let Some(op) = f.opacity {
                        if !op.is_finite() {
                            return Status::error("INVALID_OPACITY", &b.id);
                        }
                    }
                    if let Some(mult) = f.multiply {
                        for &c in &mult {
                            if !c.is_finite() {
                                return Status::error("INVALID_COLOR", &b.id);
                            }
                        }
                    }
                    if let Some(scr) = f.screen {
                        for &c in &scr {
                            if !c.is_finite() {
                                return Status::error("INVALID_COLOR", &b.id);
                            }
                        }
                    }
                }
            }
            (BlendShapeTargetKind::Part, DeltaKeyforms::Part(forms)) => {
                if !self.parts.contains_key(&b.target_id) {
                    return Status::error(
                        "MISSING_OBJECT",
                        format!("{}: expected Part", b.target_id),
                    );
                }
                for f in forms {
                    let s = validate_draw_order(f.draw_order, &b.id);
                    if !s.is_ok() {
                        return s;
                    }
                }
            }
            (BlendShapeTargetKind::Glue, DeltaKeyforms::Glue(forms)) => {
                if !self.glues.contains_key(&b.target_id) {
                    return Status::error(
                        "MISSING_OBJECT",
                        format!("{}: expected Glue", b.target_id),
                    );
                }
                for f in forms {
                    if !f.intensity.is_finite() {
                        return Status::error("INVALID_INTENSITY", &b.id);
                    }
                }
            }
            (BlendShapeTargetKind::Offscreen, DeltaKeyforms::Offscreen(forms)) => {
                if !self.offscreens.contains_key(&b.target_id) {
                    return Status::error(
                        "MISSING_OBJECT",
                        format!("{}: expected Offscreen", b.target_id),
                    );
                }
                for f in forms {
                    if !f.opacity.is_finite() {
                        return Status::error("INVALID_OPACITY", &b.id);
                    }
                    if let Some(mult) = f.multiply {
                        for &c in &mult {
                            if !c.is_finite() {
                                return Status::error("INVALID_COLOR", &b.id);
                            }
                        }
                    }
                    if let Some(scr) = f.screen {
                        for &c in &scr {
                            if !c.is_finite() {
                                return Status::error("INVALID_COLOR", &b.id);
                            }
                        }
                    }
                }
            }
            _ => {
                return Status::error(
                    "MISMATCHED_TARGET_KIND",
                    format!(
                        "{}: target_kind {:?} does not match keyform type",
                        b.id, b.target_kind
                    ),
                );
            }
        }
        Status::ok()
    }

    pub fn create_blend_binding(&mut self, b: BlendShapeBinding) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &b.id));
        }
        if self.contains_id(&b.id) {
            return self.failed(Status::error("DUPLICATE_ID", &b.id));
        }
        let s = self.validate_blend_binding(&b);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = b.id.clone();
        let target = b.target_id.clone();
        let key_table = b.key_table_id.clone();
        self.blend_bindings.insert(id.clone(), b);
        self.blend_binding_order.push(id.clone());
        let meshes = self.mesh_order.clone();
        self.changed(ChangeKind::Structure, meshes, vec![id, target, key_table])
    }

    pub fn replace_blend_binding(&mut self, b: BlendShapeBinding) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error("TRANSACTION_ACTIVE", &b.id));
        }
        if !self.blend_bindings.contains_key(&b.id) {
            return self.failed(Status::error("MISSING_BINDING", &b.id));
        }
        let s = self.validate_blend_binding(&b);
        if !s.is_ok() {
            return self.failed(s);
        }
        let id = b.id.clone();
        let target = b.target_id.clone();
        let key_table = b.key_table_id.clone();
        self.blend_bindings.insert(id.clone(), b);
        let meshes = self.mesh_order.clone();
        self.changed(ChangeKind::Structure, meshes, vec![id, target, key_table])
    }
}
