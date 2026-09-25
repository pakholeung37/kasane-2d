//! Expression stage: owns playback cursors, but no document, clock or frame.
use crate::AnimationError;
use kasane_core::{
    document::{ExpressionBlend, ExpressionTarget},
    Document,
};
use std::collections::BTreeMap;
#[derive(Debug, Clone)]
struct Activation {
    time: f32,
    expression_id: String,
}

#[derive(Debug, Clone)]
struct PlayingExpression {
    id: String,
    fade_in_start: Option<f32>,
    fade_out_triggered: bool,
    end_time: Option<f32>,
}

#[derive(Debug, Clone, Copy)]
struct Components {
    additive: f32,
    multiply: f32,
    overwrite: f32,
}

#[derive(Clone, Default)]
pub(crate) struct ExpressionRuntime {
    activations: Vec<Activation>,
    next_activation: usize,
    playing: Vec<PlayingExpression>,
}
impl ExpressionRuntime {
    /// Schedule a start at a nonnegative preview time. Ties retain call order.
    pub(crate) fn schedule(
        &mut self,
        document: &Document,
        now: f32,
        id: &str,
        time: f32,
    ) -> Result<(), AnimationError> {
        if !time.is_finite() || time < 0.0 {
            return Err(AnimationError::InvalidTime);
        }
        if time < now {
            return Err(AnimationError::PastActivation);
        }
        let expression = document
            .get_expression(id)
            .ok_or_else(|| AnimationError::MissingExpression(id.into()))?;
        for entry in &expression.entries {
            if let ExpressionTarget::Unresolved { runtime_id } = &entry.target {
                return Err(AnimationError::UnresolvedParameter {
                    expression_id: id.into(),
                    runtime_id: runtime_id.clone(),
                });
            }
        }
        let insertion = self.activations.partition_point(|item| item.time <= time);
        self.activations.insert(
            insertion,
            Activation {
                time,
                expression_id: id.into(),
            },
        );
        Ok(())
    }

    pub(crate) fn reset(&mut self) {
        self.next_activation = 0;
        self.playing.clear();
    }
    pub(crate) fn active_ids(&self) -> Vec<String> {
        self.playing.iter().map(|entry| entry.id.clone()).collect()
    }
    pub(crate) fn evaluate(
        &mut self,
        document: &Document,
        time: f32,
        values: &mut BTreeMap<String, f32>,
    ) {
        while self.next_activation < self.activations.len()
            && self.activations[self.next_activation].time <= time
        {
            for item in &mut self.playing {
                item.fade_out_triggered = true;
            }
            self.playing.push(PlayingExpression {
                id: self.activations[self.next_activation].expression_id.clone(),
                fade_in_start: None,
                fade_out_triggered: false,
                end_time: None,
            });
            self.next_activation += 1;
        }
        self.update_expressions(document, time, values);
    }
    fn update_expressions(
        &mut self,
        document: &Document,
        time: f32,
        values: &mut BTreeMap<String, f32>,
    ) {
        let mut components = BTreeMap::<String, Components>::new();
        let mut expression_weight = 0.0_f32;
        let mut latest_fade = 0.0_f32;
        for (index, playing) in self.playing.iter_mut().enumerate() {
            let expression = document
                .get_expression(&playing.id)
                .expect("scheduled expression exists");
            let start = *playing.fade_in_start.get_or_insert(time);
            let fade_in = expression.fade_in.unwrap_or(1.0);
            let fade_out = expression.fade_out.unwrap_or(1.0);
            let in_weight = if fade_in == 0.0 {
                1.0
            } else {
                ease((time - start) / fade_in)
            };
            let out_weight = match playing.end_time {
                Some(end) if fade_out != 0.0 => ease((end - time) / fade_out),
                _ => 1.0,
            };
            let weight = in_weight * out_weight;
            expression_weight += in_weight;
            latest_fade = weight;
            for entry in &expression.entries {
                if let ExpressionTarget::Resolved { parameter_id } = &entry.target {
                    components
                        .entry(parameter_id.clone())
                        .or_insert_with(|| Components {
                            additive: 0.0,
                            multiply: 1.0,
                            overwrite: *values
                                .get(parameter_id)
                                .expect("resolved parameter exists"),
                        });
                }
            }
            for (id, slot) in &mut components {
                let current = *values.get(id).expect("resolved parameter exists");
                // Framework's manager finds the first matching entry.
                let entry = expression.entries.iter().find(|entry| {
                    matches!(&entry.target, ExpressionTarget::Resolved { parameter_id } if parameter_id == id)
                });
                let desired = entry.map_or(
                    Components {
                        additive: 0.0,
                        multiply: 1.0,
                        overwrite: current,
                    },
                    |entry| match entry.blend.unwrap_or(ExpressionBlend::Add) {
                        ExpressionBlend::Add => Components {
                            additive: entry.value,
                            multiply: 1.0,
                            overwrite: current,
                        },
                        ExpressionBlend::Multiply => Components {
                            additive: 0.0,
                            multiply: entry.value,
                            overwrite: current,
                        },
                        ExpressionBlend::Overwrite => Components {
                            additive: 0.0,
                            multiply: 1.0,
                            overwrite: entry.value,
                        },
                    },
                );
                if index == 0 {
                    *slot = desired;
                } else {
                    slot.additive = lerp(slot.additive, desired.additive, weight);
                    slot.multiply = lerp(slot.multiply, desired.multiply, weight);
                    // The Framework refreshes its local OverwriteValue from
                    // the model at each queue entry, so an earlier overwrite
                    // does not carry into a later expression's fade.
                    slot.overwrite = lerp(current, desired.overwrite, weight);
                }
            }
            if playing.fade_out_triggered && playing.end_time.is_none() {
                playing.end_time = Some(time + fade_out);
            }
        }
        if self.playing.len() > 1 && latest_fade >= 1.0 {
            let latest = self.playing.pop().expect("nonempty queue");
            self.playing.clear();
            self.playing.push(latest);
        }
        let expression_weight = expression_weight.min(1.0);
        for (id, slot) in components {
            let base = *values.get(&id).expect("resolved parameter exists");
            let desired = (slot.overwrite + slot.additive) * slot.multiply;
            let parameter = document
                .get_parameter(&id)
                .expect("resolved parameter exists");
            values.insert(
                id,
                lerp(base, desired, expression_weight).clamp(parameter.minimum, parameter.maximum),
            );
        }
    }
}

fn ease(value: f32) -> f32 {
    if value <= 0.0 {
        0.0
    } else if value >= 1.0 {
        1.0
    } else {
        0.5 - 0.5 * (value * std::f32::consts::PI).cos()
    }
}

fn lerp(from: f32, to: f32, weight: f32) -> f32 {
    from * (1.0 - weight) + to * weight
}
