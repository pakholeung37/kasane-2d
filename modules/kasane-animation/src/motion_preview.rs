//! Detached MotionBehavior V2 preview for typed project clips.
use std::{collections::BTreeMap, sync::Arc};

use kasane_core::{evaluate_frame, Document, DrawableFrame, PreviewValues};

use crate::expression::ExpressionRuntime;
use crate::motion_runtime::MotionRuntime;
use crate::physics::PhysicsRuntime;
use crate::pose::PoseRuntime;
use crate::{AnimationError, CompiledCurve};

#[derive(Debug, Clone, PartialEq)]
pub struct MotionEventFired {
    pub motion_id: String,
    pub event_id: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MotionSnapshot {
    pub time: f32,
    pub parameters: BTreeMap<String, f32>,
    /// PartOpacity curves write virtual controls when no real parameter has the Part runtime ID.
    pub part_opacity_channels: BTreeMap<String, f32>,
    pub part_opacities: BTreeMap<String, f32>,
    pub model_opacity: f32,
    pub active_motions: Vec<String>,
    pub active_expressions: Vec<String>,
    pub fired_events: Vec<MotionEventFired>,
    /// Model EyeBlink/LipSync mappings and unsupported Model IDs are reported.
    pub coverage: Vec<String>,
}

pub struct MotionPreview {
    document: Arc<Document>,
    curves: Arc<BTreeMap<String, CompiledCurve>>,
    source_identity: Option<(u64, u64)>,
    base: BTreeMap<String, f32>,
    snapshot: MotionSnapshot,
    pose: PoseRuntime,
    physics: PhysicsRuntime,
    expressions: ExpressionRuntime,
    motion: MotionRuntime,
}

impl MotionPreview {
    pub fn new(document: &Document) -> Self {
        let curves = document
            .motion_order()
            .iter()
            .flat_map(|id| &document.get_motion(id).expect("ordered motion").tracks)
            .map(|track| (track.id.clone(), CompiledCurve::from(track)))
            .collect();
        Self::from_shared(Arc::new(document.clone()), Arc::new(curves))
    }

    fn from_shared(document: Arc<Document>, curves: Arc<BTreeMap<String, CompiledCurve>>) -> Self {
        let base: BTreeMap<String, f32> = document
            .parameter_order()
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    document
                        .get_parameter(id)
                        .expect("ordered parameter")
                        .default_value,
                )
            })
            .collect();
        let mut parameters = base.clone();
        let mut virtual_controls = BTreeMap::new();
        let pose = PoseRuntime::new(&document, &mut parameters, &mut virtual_controls);
        Self {
            document: Arc::clone(&document),
            curves,
            source_identity: None,
            motion: MotionRuntime::new(parameters.clone()),
            physics: PhysicsRuntime::new(&document),
            expressions: ExpressionRuntime::default(),
            base,
            snapshot: MotionSnapshot {
                time: 0.0,
                parameters,
                part_opacity_channels: virtual_controls,
                part_opacities: pose.opacities.clone(),
                model_opacity: 1.0,
                active_motions: Vec::new(),
                active_expressions: Vec::new(),
                fired_events: Vec::new(),
                coverage: Vec::new(),
            },
            pose,
        }
    }
    pub fn document_revision(&self) -> u64 {
        self.document.revision()
    }
    pub fn bind_session_identity(&mut self, session_id: u64, generation: u64) {
        self.source_identity = Some((session_id, generation));
    }
    pub fn source_identity(&self) -> Option<(u64, u64)> {
        self.source_identity
    }
    pub fn document_id(&self) -> &str {
        self.document.id()
    }
    pub fn snapshot(&self) -> &MotionSnapshot {
        &self.snapshot
    }

    pub fn schedule_motion(&mut self, id: &str, time: f32) -> Result<(), AnimationError> {
        self.motion
            .schedule_motion(&self.document, self.snapshot.time, id, time)
    }

    pub fn schedule_expression(&mut self, id: &str, time: f32) -> Result<(), AnimationError> {
        self.expressions
            .schedule(&self.document, self.snapshot.time, id, time)
    }

    pub fn set_base_parameter(&mut self, id: &str, value: f32) -> Result<(), AnimationError> {
        let parameter = self
            .document
            .get_parameter(id)
            .ok_or_else(|| AnimationError::Evaluation(format!("missing parameter {id}")))?;
        if !value.is_finite() {
            return Err(AnimationError::Evaluation(format!(
                "nonfinite parameter {id}"
            )));
        }
        self.base
            .insert(id.into(), value.clamp(parameter.minimum, parameter.maximum));
        self.reset();
        Ok(())
    }

    /// Schedule an editor override retained by reset and seek.
    pub fn schedule_parameter_input(
        &mut self,
        id: &str,
        time: f32,
        value: f32,
    ) -> Result<(), AnimationError> {
        self.motion
            .schedule_parameter_input(&self.document, self.snapshot.time, id, time, value)
    }

    pub fn reset(&mut self) {
        let mut parameters = self.base.clone();
        let mut virtual_controls = BTreeMap::new();
        self.pose
            .reset(&self.document, &mut parameters, &mut virtual_controls);
        self.physics.reset(&self.document);
        self.expressions.reset();
        self.motion.reset(parameters.clone());
        self.snapshot = MotionSnapshot {
            time: 0.0,
            parameters,
            part_opacity_channels: virtual_controls,
            part_opacities: self.pose.opacities.clone(),
            model_opacity: 1.0,
            active_motions: Vec::new(),
            active_expressions: Vec::new(),
            fired_events: Vec::new(),
            coverage: Vec::new(),
        };
    }

    pub fn advance(&mut self, dt: f32) -> Result<&MotionSnapshot, AnimationError> {
        if !dt.is_finite() || dt < 0.0 || !(self.snapshot.time + dt).is_finite() {
            return Err(AnimationError::InvalidTime);
        }
        let time = self.snapshot.time + dt;
        self.motion
            .evaluate(&self.document, &self.curves, time, &mut self.snapshot)?;
        self.expressions
            .evaluate(&self.document, time, &mut self.snapshot.parameters);
        self.snapshot.active_expressions = self.expressions.active_ids();
        self.physics.evaluate(
            &self.document,
            &mut self.snapshot.parameters,
            dt,
            &mut self.snapshot.coverage,
        );
        self.pose.update(
            &self.document,
            &self.snapshot.parameters,
            &self.snapshot.part_opacity_channels,
            dt,
            &mut self.snapshot.coverage,
        );
        self.snapshot.part_opacities = self.pose.opacities.clone();
        self.snapshot.time = time;
        self.snapshot.coverage.sort();
        self.snapshot.coverage.dedup();
        Ok(&self.snapshot)
    }

    pub fn stabilize_physics(&mut self) {
        self.physics
            .stabilize(&self.document, &mut self.snapshot.parameters);
    }

    pub fn seek(&mut self, time: f32) -> Result<&MotionSnapshot, AnimationError> {
        self.seek_with_progress(time, |_, _| true)
    }

    /// Replay on a candidate state, reporting completed and total steps.
    /// Returning false cancels the seek without changing the current preview.
    pub fn seek_with_progress(
        &mut self,
        time: f32,
        mut progress: impl FnMut(u32, u32) -> bool,
    ) -> Result<&MotionSnapshot, AnimationError> {
        let steps = crate::replay::ReplaySteps::new(time)?;
        let mut candidate = Self::from_shared(Arc::clone(&self.document), Arc::clone(&self.curves));
        candidate.source_identity = self.source_identity;
        candidate.base.clone_from(&self.base);
        candidate.motion.copy_schedule_from(&self.motion);
        candidate.expressions.clone_from(&self.expressions);
        candidate.reset();
        let total = steps.total();
        if !progress(0, total) {
            return Err(AnimationError::SeekCancelled);
        }
        for (index, next) in steps.enumerate() {
            candidate.advance((next - candidate.snapshot.time).max(0.0))?;
            if !progress(index as u32 + 1, total) {
                return Err(AnimationError::SeekCancelled);
            }
        }
        *self = candidate;
        Ok(&self.snapshot)
    }

    pub fn evaluate_drawables(&self) -> Result<DrawableFrame, AnimationError> {
        let values: PreviewValues = self
            .snapshot
            .parameters
            .iter()
            .map(|(id, value)| (id.clone(), *value))
            .collect();
        let mut frame = DrawableFrame::default();
        let status = evaluate_frame(&self.document, &values, &mut frame);
        if status.is_ok() {
            // Core evaluation does not carry runtime Part opacity. Apply it to
            // each drawable once; Offscreen opacity is an independent factor.
            for drawable in &mut frame.drawables {
                let mut part_id = drawable.part_id.as_str();
                let mut opacity = 1.0;
                while !part_id.is_empty() {
                    opacity *= self
                        .snapshot
                        .part_opacities
                        .get(part_id)
                        .copied()
                        .unwrap_or(1.0);
                    part_id = self
                        .document
                        .get_part(part_id)
                        .map_or("", |part| part.parent_id.as_str());
                }
                drawable.opacity *= opacity;
            }
            Ok(frame)
        } else {
            Err(AnimationError::Evaluation(format!(
                "{}: {}",
                status.code, status.message
            )))
        }
    }
}
