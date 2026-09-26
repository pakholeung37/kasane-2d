//! Detached MotionBehavior V2 preview for typed project clips.
use std::{collections::BTreeMap, sync::Arc};

use kasane_core::{Document, DrawableFrame, EvaluationTrace, FrameEvaluator, PreviewValues};

use crate::expression::ExpressionRuntime;
use crate::motion_runtime::MotionRuntime;
use crate::physics::PhysicsRuntime;
use crate::pose::PoseRuntime;
use crate::seek_cache::{Checkpoint, SeekCache};
use crate::{AnimationError, CompiledCurve, SeekCacheStats};

/// A motion event crossed during the last update, returned without side effects.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MotionEventFired {
    /// Source clip UUID.
    pub motion_id: String,
    /// Source event UUID.
    pub event_id: String,
    /// User-authored event text.
    pub value: String,
}

/// Combined values after Motion, Expression, Physics and Pose evaluation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MotionSnapshot {
    /// Absolute preview time in seconds.
    pub time: f32,
    /// Final parameter values keyed by parameter UUID.
    pub parameters: BTreeMap<String, f32>,
    /// PartOpacity curves write virtual controls when no real parameter has the Part runtime ID.
    pub part_opacity_channels: BTreeMap<String, f32>,
    /// Pose-produced opacities keyed by Part UUID.
    pub part_opacities: BTreeMap<String, f32>,
    /// Model Opacity channel, separate from the drawable geometry opacity.
    pub model_opacity: f32,
    /// Active clip UUIDs in queue order; repeated activations may repeat a UUID.
    pub active_motions: Vec<String>,
    /// Active expression UUIDs in queue order, including fading-out entries.
    pub active_expressions: Vec<String>,
    /// Events crossed by the last update, not accumulated across all seek steps.
    pub fired_events: Vec<MotionEventFired>,
    /// Model EyeBlink/LipSync mappings and unsupported Model IDs are reported.
    pub coverage: Vec<String>,
}

/// Identity of the last successful preview operation. This is evidence of
/// provenance, not a serialized simulation state or a replay recipe.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MotionOperation {
    pub preview_id: String,
    pub sequence: u64,
    pub kind: MotionOperationKind,
    pub actual_time: f32,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum MotionOperationKind {
    Created,
    Reset,
    SetBaseParameter {
        parameter_id: String,
        value: f32,
    },
    ScheduleMotion {
        motion_id: String,
        time: f32,
    },
    ScheduleMotionEntry {
        group: String,
        index: usize,
        motion_id: String,
        time: f32,
    },
    ScheduleExpression {
        expression_id: String,
        time: f32,
    },
    ScheduleParameterInput {
        parameter_id: String,
        time: f32,
        value: f32,
    },
    Advance {
        dt: f32,
    },
    Seek {
        requested_time: f32,
    },
    StabilizePhysics,
}

/// Detached combined preview with immutable assets and bounded canonical seek checkpoints.
/// Create a new preview after authoring edits; playback never edits the source document.
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
    cache: SeekCache,
    operation: MotionOperation,
}

impl MotionPreview {
    /// Capture an independent document snapshot at time zero. Later document edits are not observed.
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
            cache: SeekCache::default(),
            operation: MotionOperation {
                preview_id: uuid::Uuid::new_v4().to_string(),
                sequence: 0,
                kind: MotionOperationKind::Created,
                actual_time: 0.0,
            },
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
    /// Return the captured document revision, not the current authoring-session revision.
    pub fn document_revision(&self) -> u64 {
        self.document.revision()
    }
    /// Associate the preview with an observer session and generation.
    /// This metadata does not change playback, schedules or cached checkpoints.
    pub fn bind_session_identity(&mut self, session_id: u64, generation: u64) {
        self.source_identity = Some((session_id, generation));
    }
    /// Return the observer session/generation, or `None` for an unbound preview.
    pub fn source_identity(&self) -> Option<(u64, u64)> {
        self.source_identity
    }
    /// Return the captured document UUID.
    pub fn document_id(&self) -> &str {
        self.document.id()
    }
    /// Borrow the current snapshot without advancing playback.
    pub fn snapshot(&self) -> &MotionSnapshot {
        &self.snapshot
    }

    pub fn operation(&self) -> &MotionOperation {
        &self.operation
    }

    fn commit_operation(&mut self, kind: MotionOperationKind) {
        self.operation.sequence += 1;
        self.operation.kind = kind;
        self.operation.actual_time = self.snapshot.time;
    }

    /// Schedule a clip UUID using clip fades (no model3 registration overrides).
    /// Time is finite, nonnegative absolute seconds and cannot precede current time.
    /// Equal-time calls retain order; activation starts on the first update at or after time.
    /// Returns `InvalidTime`, `PastActivation` or `MissingMotion` on invalid input.
    /// Unresolved imported tracks are skipped and reported in snapshot coverage.
    /// Success clears seek checkpoints/statistics; failure changes nothing.
    pub fn schedule_motion(&mut self, id: &str, time: f32) -> Result<(), AnimationError> {
        self.motion
            .schedule_motion(&self.document, self.snapshot.time, id, time, (None, None))?;
        self.cache.clear();
        self.commit_operation(MotionOperationKind::ScheduleMotion {
            motion_id: id.into(),
            time,
        });
        Ok(())
    }

    /// Schedule a model3 group entry at an absolute preview time in seconds.
    ///
    /// `group` is an exact group name and `index` is zero-based. Parameter-curve
    /// fades override registration fades, which override clip fades (default:
    /// one second). Each activation retains its own fades. Sound is not played.
    /// Success clears seek checkpoints and statistics without resetting playback.
    ///
    /// # Errors
    /// Returns [`AnimationError::MissingMotionEntry`] for an unknown group/index,
    /// [`AnimationError::InvalidTime`] for nonfinite or negative time,
    /// or [`AnimationError::PastActivation`] for time before the current preview.
    /// Unresolved MOC targets remain in the clip and are reported as coverage
    /// diagnostics when sampled, since Cubism permits virtual parameters.
    /// Failure leaves playback, schedules and the cache unchanged.
    pub fn schedule_motion_entry(
        &mut self,
        group: &str,
        index: usize,
        time: f32,
    ) -> Result<(), AnimationError> {
        let entry = self
            .document
            .motion_groups()
            .iter()
            .find(|item| item.name == group)
            .and_then(|item| item.entries.get(index))
            .ok_or_else(|| AnimationError::MissingMotionEntry {
                group: group.into(),
                index,
            })?;
        self.motion.schedule_motion(
            &self.document,
            self.snapshot.time,
            &entry.clip_id,
            time,
            (entry.fade_in, entry.fade_out),
        )?;
        self.cache.clear();
        self.commit_operation(MotionOperationKind::ScheduleMotionEntry {
            group: group.into(),
            index,
            motion_id: entry.clip_id.clone(),
            time,
        });
        Ok(())
    }

    /// Set the retained checkpoint budget in bytes (default: 16 MiB).
    ///
    /// Every call clears checkpoints and seek statistics, even if the budget is
    /// unchanged; zero disables caching. Playback and schedules are preserved.
    /// The budget uses conservative allocation estimates, excluding shared
    /// document/curve data and temporary transactional seek state. Oldest inserted
    /// checkpoints are evicted first; a checkpoint larger than the budget is skipped.
    pub fn set_seek_cache_budget(&mut self, bytes: usize) {
        self.cache.set_budget(bytes);
    }
    /// Discard all checkpoints and zero seek statistics, retaining the budget.
    /// Playback, baseline values and schedules are unchanged.
    pub fn clear_seek_cache(&mut self) {
        self.cache.clear();
    }
    /// Return a detached copy of cache usage and last successful seek statistics.
    /// Reading statistics does not mutate playback or the cache.
    pub fn seek_cache_stats(&self) -> SeekCacheStats {
        self.cache.stats
    }

    fn checkpoint_bytes(&self) -> usize {
        use crate::seek_cache::{map_bytes, strings_bytes, vec_bytes};
        // Include Arc and cache-entry allocation overhead in each estimate.
        std::mem::size_of::<Checkpoint>()
            + 128
            + map_bytes(&self.snapshot.parameters)
            + map_bytes(&self.snapshot.part_opacity_channels)
            + map_bytes(&self.snapshot.part_opacities)
            + strings_bytes(&self.snapshot.active_motions)
            + strings_bytes(&self.snapshot.active_expressions)
            + strings_bytes(&self.snapshot.coverage)
            + vec_bytes(&self.snapshot.fired_events)
            + self
                .snapshot
                .fired_events
                .iter()
                .map(|e| e.motion_id.capacity() + e.event_id.capacity() + e.value.capacity())
                .sum::<usize>()
            + self.motion.heap_bytes()
            + self.expressions.heap_bytes()
            + self.physics.heap_bytes()
            + self.pose.heap_bytes()
    }

    /// Schedule an expression UUID at an absolute time in seconds; equal-time calls retain order.
    /// Activation starts on the first update at or after the scheduled time.
    /// Returns `InvalidTime` for nonfinite/negative time, `PastActivation` for past time,
    /// `MissingExpression` for an unknown UUID, or `UnresolvedParameter` for unresolved
    /// targets. Failure preserves state; Motion preview clears its cache on success.
    pub fn schedule_expression(&mut self, id: &str, time: f32) -> Result<(), AnimationError> {
        self.expressions
            .schedule(&self.document, self.snapshot.time, id, time)?;
        self.cache.clear();
        self.commit_operation(MotionOperationKind::ScheduleExpression {
            expression_id: id.into(),
            time,
        });
        Ok(())
    }

    /// Set a parameter UUID baseline, clamp it to its range, then reset playback.
    /// Retains schedules. Missing UUIDs/nonfinite values return [`AnimationError::Evaluation`]
    /// without changing state. Motion preview also clears seek checkpoints/statistics.
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
        let value = value.clamp(parameter.minimum, parameter.maximum);
        self.base.insert(id.into(), value);
        self.reset_state();
        self.commit_operation(MotionOperationKind::SetBaseParameter {
            parameter_id: id.into(),
            value,
        });
        Ok(())
    }

    /// Schedule a parameter UUID override retained by reset and seek.
    /// Time is absolute seconds, finite, nonnegative and no earlier than current time.
    /// Equal-time inputs retain call order and apply before Motion on the first due update.
    /// Values must be finite and are clamped to the parameter range. Invalid time/value
    /// returns `InvalidTime`, past time `PastActivation`, and missing UUID `Evaluation`.
    /// Success clears seek checkpoints/statistics; failure preserves state.
    pub fn schedule_parameter_input(
        &mut self,
        id: &str,
        time: f32,
        value: f32,
    ) -> Result<(), AnimationError> {
        self.motion.schedule_parameter_input(
            &self.document,
            self.snapshot.time,
            id,
            time,
            value,
        )?;
        self.cache.clear();
        self.commit_operation(MotionOperationKind::ScheduleParameterInput {
            parameter_id: id.into(),
            time,
            value,
        });
        Ok(())
    }

    /// Restore time zero and initial Motion/Expression/Physics/Pose state from the baseline.
    /// Retains activation/input schedules and cache budget; clears checkpoints/statistics
    /// and fired events. Time-zero activations require `advance(0)` or `seek(0)`.
    pub fn reset(&mut self) {
        self.reset_state();
        self.commit_operation(MotionOperationKind::Reset);
    }

    fn reset_state(&mut self) {
        self.cache.clear();
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

    /// Advance Motion → Expression → Physics → Pose by finite, nonnegative seconds.
    /// Zero evaluates activations due now. Invalid delta/clock overflow returns
    /// `InvalidTime`; an estimated event batch over one million returns `EventLimit`.
    /// These failures preserve playback. Arbitrary advances do not populate seek checkpoints.
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
        self.commit_operation(MotionOperationKind::Advance { dt });
        Ok(&self.snapshot)
    }

    /// Initialize particles and physics outputs using current snapshot parameters.
    /// Does not advance time or run Motion/Expression/Pose. Does not seed checkpoints;
    /// subsequent seek still replays the original baseline and scheduled inputs.
    pub fn stabilize_physics(&mut self) {
        self.physics
            .stabilize(&self.document, &mut self.snapshot.parameters);
        self.commit_operation(MotionOperationKind::StabilizePhysics);
    }

    /// Seek to absolute seconds using canonical checkpoints or the initial state.
    /// Uses an absolute 60 Hz grid and a partial final step. Cold replay begins
    /// with a zero-length step at time zero. Nonfinite/negative time returns `InvalidTime`; time * 60
    /// above one million returns `SeekLimit`, even with cached state. Failure preserves
    /// playback and cache. Returned events are data, with no playback side effects.
    pub fn seek(&mut self, time: f32) -> Result<&MotionSnapshot, AnimationError> {
        self.seek_with_progress(time, |_, _| true)
    }

    /// Replay from a canonical checkpoint, reporting remaining completed/total steps.
    /// An exact cache hit reports (0, 0).
    /// Returning false produces `SeekCancelled`, preserving playback and cache/statistics.
    /// The initial callback runs before replay, followed by one per replay step.
    /// Time constraints and other errors are the same as [`Self::seek`].
    pub fn seek_with_progress(
        &mut self,
        time: f32,
        mut progress: impl FnMut(u32, u32) -> bool,
    ) -> Result<&MotionSnapshot, AnimationError> {
        let mut steps = crate::replay::ReplaySteps::new(time)?;
        let mut candidate = Self::from_shared(Arc::clone(&self.document), Arc::clone(&self.curves));
        candidate.source_identity = self.source_identity;
        candidate
            .operation
            .preview_id
            .clone_from(&self.operation.preview_id);
        candidate.base.clone_from(&self.base);
        candidate.motion.copy_schedule_from(&self.motion);
        candidate.expressions.clone_from(&self.expressions);
        candidate.reset_state();
        let mut cache = self.cache.clone();
        let checkpoint = cache.nearest(time);
        let start = checkpoint.as_ref().map_or(0, |state| state.step);
        if let Some(state) = checkpoint {
            candidate.snapshot.clone_from(&state.snapshot);
            candidate.motion.clone_from(&state.motion);
            candidate.expressions.clone_from(&state.expressions);
            candidate.physics.clone_from(&state.physics);
            candidate.pose.clone_from(&state.pose);
        }
        let total = steps.total() - start;
        cache.stats.last_restored_time = candidate.snapshot.time;
        cache.stats.last_replayed_steps = total;
        if !progress(0, total) {
            return Err(AnimationError::SeekCancelled);
        }
        steps.resume_after(start);
        for (index, next) in steps.enumerate() {
            candidate.advance((next - candidate.snapshot.time).max(0.0))?;
            let step = start + index as u32 + 1;
            // Never retain partial tail steps, seek(0), or arbitrary advance state.
            let grid_step = step - 1; // The first evaluation is time zero.
            if grid_step > 0 && grid_step.is_multiple_of(60) && next == grid_step as f32 / 60.0 {
                cache.insert(candidate.checkpoint_bytes(), || Checkpoint {
                    step,
                    snapshot: candidate.snapshot.clone(),
                    motion: candidate.motion.clone(),
                    expressions: candidate.expressions.clone(),
                    physics: candidate.physics.clone(),
                    pose: candidate.pose.clone(),
                });
            }
            if !progress(step - start, total) {
                return Err(AnimationError::SeekCancelled);
            }
        }
        candidate.cache = cache;
        candidate.operation.sequence = self.operation.sequence;
        candidate.commit_operation(MotionOperationKind::Seek {
            requested_time: time,
        });
        *self = candidate;
        Ok(&self.snapshot)
    }

    /// Evaluate geometry and multiply each drawable by its ancestor Part opacities.
    /// Model opacity remains separate in [`MotionSnapshot::model_opacity`].
    /// Does not advance playback. Returns [`AnimationError::Evaluation`] on failure.
    pub fn evaluate_drawables(&self) -> Result<DrawableFrame, AnimationError> {
        self.evaluate_drawables_inner(false, false)
            .map(|(frame, _)| frame)
    }

    pub fn evaluate_drawables_with_trace(
        &self,
    ) -> Result<(DrawableFrame, EvaluationTrace), AnimationError> {
        self.evaluate_drawables_inner(true, false)
            .map(|(frame, trace)| (frame, trace.expect("trace requested")))
    }

    pub fn evaluate_drawables_with_trace_and_hidden_geometry(
        &self,
    ) -> Result<(DrawableFrame, EvaluationTrace), AnimationError> {
        self.evaluate_drawables_inner(true, true)
            .map(|(frame, trace)| (frame, trace.expect("trace requested")))
    }

    fn evaluate_drawables_inner(
        &self,
        with_trace: bool,
        include_hidden_geometry: bool,
    ) -> Result<(DrawableFrame, Option<EvaluationTrace>), AnimationError> {
        let values: PreviewValues = self
            .snapshot
            .parameters
            .iter()
            .map(|(id, value)| (id.clone(), *value))
            .collect();
        let mut frame = DrawableFrame::default();
        let mut trace = EvaluationTrace::default();
        let mut evaluator = FrameEvaluator::default();
        let status = if with_trace {
            evaluator.evaluate_with_trace_and_hidden_geometry(
                &self.document,
                &values,
                &mut frame,
                &mut trace,
                include_hidden_geometry,
            )
        } else {
            evaluator.evaluate(&self.document, &values, &mut frame)
        };
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
            Ok((frame, with_trace.then_some(trace)))
        } else {
            Err(AnimationError::Evaluation(format!(
                "{}: {}",
                status.code, status.message
            )))
        }
    }
}
