//! Motion stage. The owner supplies one clock and frame for all stages.
use crate::pose::real_parameter_for_part;
use crate::{sample_motion_curve, AnimationError, CompiledCurve, MotionEventFired, MotionSnapshot};
use kasane_core::{
    document::{MotionClip, MotionSegment, MotionTrack, MotionTrackTarget},
    Document,
};
use std::collections::BTreeMap;
#[derive(Clone)]
struct Activation {
    time: f32,
    motion_id: String,
    fades: (f32, f32),
}

#[derive(Clone)]
struct ParameterInput {
    time: f32,
    parameter_id: String,
    value: f32,
}

#[derive(Clone)]
struct Playing {
    id: String,
    start_time: f32,
    fade_in_start: f32,
    end_time: Option<f32>,
    previous_offset: f32,
    fades: (f32, f32),
}

#[derive(Clone)]
pub(crate) struct MotionRuntime {
    motion_parameters: BTreeMap<String, f32>,
    activations: Vec<Activation>,
    parameter_inputs: Vec<ParameterInput>,
    next_parameter_input: usize,
    next_activation: usize,
    playing: Vec<Playing>,
}
impl MotionRuntime {
    pub(crate) fn heap_bytes(&self) -> usize {
        use crate::seek_cache::{map_bytes, vec_bytes};
        map_bytes(&self.motion_parameters)
            + vec_bytes(&self.activations)
            + self
                .activations
                .iter()
                .map(|v| v.motion_id.capacity())
                .sum::<usize>()
            + vec_bytes(&self.parameter_inputs)
            + self
                .parameter_inputs
                .iter()
                .map(|v| v.parameter_id.capacity())
                .sum::<usize>()
            + vec_bytes(&self.playing)
            + self.playing.iter().map(|v| v.id.capacity()).sum::<usize>()
    }
    pub(crate) fn new(parameters: BTreeMap<String, f32>) -> Self {
        Self {
            motion_parameters: parameters,
            activations: Vec::new(),
            parameter_inputs: Vec::new(),
            next_parameter_input: 0,
            next_activation: 0,
            playing: Vec::new(),
        }
    }
    pub(crate) fn copy_schedule_from(&mut self, other: &Self) {
        self.activations.clone_from(&other.activations);
        self.parameter_inputs.clone_from(&other.parameter_inputs);
    }
    pub(crate) fn reset(&mut self, parameters: BTreeMap<String, f32>) {
        self.motion_parameters = parameters;
        self.next_parameter_input = 0;
        self.next_activation = 0;
        self.playing.clear();
    }
    pub(crate) fn schedule_motion(
        &mut self,
        document: &Document,
        now: f32,
        id: &str,
        time: f32,
        fades: (Option<f32>, Option<f32>),
    ) -> Result<(), AnimationError> {
        if !time.is_finite() || time < 0.0 {
            return Err(AnimationError::InvalidTime);
        }
        if time < now {
            return Err(AnimationError::PastActivation);
        }
        let clip = document
            .get_motion(id)
            .ok_or_else(|| AnimationError::MissingMotion(id.into()))?;
        let index = self.activations.partition_point(|item| item.time <= time);
        self.activations.insert(
            index,
            Activation {
                time,
                motion_id: id.into(),
                fades: (
                    fades.0.or(clip.fade_in).unwrap_or(1.0),
                    fades.1.or(clip.fade_out).unwrap_or(1.0),
                ),
            },
        );
        Ok(())
    }

    /// Schedule an editor input override. The schedule is retained by reset
    /// and replayed by seek, including the Physics input cache history.
    pub(crate) fn schedule_parameter_input(
        &mut self,
        document: &Document,
        now: f32,
        id: &str,
        time: f32,
        value: f32,
    ) -> Result<(), AnimationError> {
        if !time.is_finite() || time < 0.0 || !value.is_finite() {
            return Err(AnimationError::InvalidTime);
        }
        if time < now {
            return Err(AnimationError::PastActivation);
        }
        let parameter = document
            .get_parameter(id)
            .ok_or_else(|| AnimationError::Evaluation(format!("missing parameter {id}")))?;
        let index = self
            .parameter_inputs
            .partition_point(|input| input.time <= time);
        self.parameter_inputs.insert(
            index,
            ParameterInput {
                time,
                parameter_id: id.into(),
                value: value.clamp(parameter.minimum, parameter.maximum),
            },
        );
        Ok(())
    }

    pub(crate) fn evaluate(
        &mut self,
        document: &Document,
        curves: &BTreeMap<String, CompiledCurve>,
        time: f32,
        snapshot: &mut MotionSnapshot,
    ) -> Result<(), AnimationError> {
        // Bound event expansion before mutating state. Empty-event loops can
        // still sample arbitrarily large deltas in constant time.
        let mut event_bound = 0.0f64;
        for playing in &self.playing {
            let clip = document.get_motion(&playing.id).expect("scheduled motion");
            if clip.looping && !clip.events.is_empty() {
                let cycle = f64::from(clip.duration + 1.0 / clip.fps);
                event_bound += ((f64::from(time - playing.start_time) / cycle).ceil() + 1.0)
                    * clip.events.len() as f64;
            }
        }
        if event_bound > 1_000_000.0 {
            return Err(AnimationError::EventLimit);
        }
        while self.next_parameter_input < self.parameter_inputs.len()
            && self.parameter_inputs[self.next_parameter_input].time <= time
        {
            let input = &self.parameter_inputs[self.next_parameter_input];
            self.motion_parameters
                .insert(input.parameter_id.clone(), input.value);
            self.next_parameter_input += 1;
        }
        snapshot.parameters = self.motion_parameters.clone();
        snapshot.fired_events.clear();
        snapshot.coverage.clear();
        while self.next_activation < self.activations.len()
            && self.activations[self.next_activation].time <= time
        {
            for old in &mut self.playing {
                let fade_out = old.fades.1;
                old.end_time = Some(
                    old.end_time
                        .map_or(time + fade_out, |end| end.min(time + fade_out)),
                );
            }
            let clip = document
                .get_motion(&self.activations[self.next_activation].motion_id)
                .expect("scheduled motion");
            self.playing.push(Playing {
                id: self.activations[self.next_activation].motion_id.clone(),
                start_time: time,
                fade_in_start: time,
                end_time: (!clip.looping).then_some(time + clip.duration),
                previous_offset: 0.0,
                fades: self.activations[self.next_activation].fades,
            });
            self.next_activation += 1;
        }
        for playing in &mut self.playing {
            let clip = document.get_motion(&playing.id).expect("scheduled motion");
            let (fade_in, fade_out) = playing.fades;
            let elapsed = (time - playing.start_time).max(0.0);
            let cycle = clip.duration + 1.0 / clip.fps;
            let offset = if clip.looping && elapsed > cycle {
                let remainder = elapsed % cycle;
                // Framework samples the end of a cycle at exact multiples.
                if remainder == 0.0 {
                    cycle
                } else {
                    remainder
                }
            } else {
                elapsed
            };
            let in_weight = if fade_in <= 0.0 {
                1.0
            } else {
                ease((time - playing.fade_in_start) / fade_in)
            };
            let out_weight = match playing.end_time {
                Some(end) if fade_out > 0.0 => ease((end - time) / fade_out),
                _ => 1.0,
            };
            let weight = in_weight * out_weight;
            for track in clip.tracks_in_evaluation_order() {
                let value = sample_track(track, &curves[&track.id], offset, clip);
                match &track.target {
                    MotionTrackTarget::Parameter { parameter_id } => {
                        let current = snapshot
                            .parameters
                            .get(parameter_id)
                            .copied()
                            .expect("resolved parameter");
                        let in_weight = match track.fade_in {
                            Some(fade) if fade > 0.0 => ease((time - playing.fade_in_start) / fade),
                            Some(_) => 1.0,
                            None => in_weight,
                        };
                        let out_weight = match (track.fade_out, playing.end_time) {
                            (Some(fade), Some(end)) if fade > 0.0 => ease((end - time) / fade),
                            (Some(_), _) => 1.0,
                            (None, _) => out_weight,
                        };
                        let parameter = document
                            .get_parameter(parameter_id)
                            .expect("resolved parameter");
                        snapshot.parameters.insert(
                            parameter_id.clone(),
                            lerp(
                                current,
                                value,
                                if track.fade_in.is_none() && track.fade_out.is_none() {
                                    weight
                                } else {
                                    in_weight * out_weight
                                },
                            )
                            .clamp(parameter.minimum, parameter.maximum),
                        );
                    }
                    MotionTrackTarget::PartOpacity { part_id } => {
                        if let Some(parameter_id) = real_parameter_for_part(document, part_id) {
                            let parameter = document
                                .get_parameter(&parameter_id)
                                .expect("resolved parameter");
                            snapshot.parameters.insert(
                                parameter_id,
                                value.clamp(parameter.minimum, parameter.maximum),
                            );
                        } else {
                            snapshot
                                .part_opacity_channels
                                .insert(part_id.clone(), value);
                        }
                        if document.pose().is_none() {
                            snapshot.coverage.push(format!(
                                "{}: PartOpacity {part_id} has no Pose asset",
                                clip.id
                            ));
                        }
                    }
                    MotionTrackTarget::Model { runtime_id } if runtime_id == "Opacity" => {
                        snapshot.model_opacity = value;
                    }
                    MotionTrackTarget::Model { runtime_id } => snapshot.coverage.push(format!(
                        "{}: Model {runtime_id} mapping requires model3 Groups",
                        clip.id
                    )),
                    MotionTrackTarget::Unresolved {
                        category,
                        runtime_id,
                    } => snapshot.coverage.push(format!(
                        "{}: virtual {category} {runtime_id} has no drawable target",
                        clip.id
                    )),
                }
            }
            let mut before = playing.previous_offset;
            let mut after = elapsed;
            if clip.looping && !clip.events.is_empty() {
                // Use unwrapped elapsed time: comparing wrapped offsets loses
                // events whenever one update spans a complete cycle or more.
                while after > cycle {
                    fire_events(clip, before, cycle, &mut snapshot.fired_events);
                    after -= cycle;
                    before = -f32::EPSILON;
                }
            }
            fire_events(clip, before, after, &mut snapshot.fired_events);
            playing.previous_offset = offset;
            if clip.looping && time - playing.start_time >= cycle {
                playing.start_time = time - offset;
                playing.previous_offset = offset;
                playing.fade_in_start = playing.start_time;
            }
            if !clip.looping && offset >= clip.duration {
                playing.end_time = Some(time);
            }
        }
        self.playing.retain(|item| {
            item.end_time.is_none_or(|end| end >= time)
                && document
                    .get_motion(&item.id)
                    .is_some_and(|clip| clip.looping || time - item.start_time < clip.duration)
        });
        self.motion_parameters = snapshot.parameters.clone();
        snapshot.active_motions = self.playing.iter().map(|item| item.id.clone()).collect();
        Ok(())
    }
}

fn fire_events(clip: &MotionClip, before: f32, after: f32, result: &mut Vec<MotionEventFired>) {
    for event in &clip.events {
        if event.time > before && event.time <= after {
            result.push(MotionEventFired {
                motion_id: clip.id.clone(),
                event_id: event.id.clone(),
                value: event.value.clone(),
            });
        }
    }
}

fn sample_track(track: &MotionTrack, curve: &CompiledCurve, time: f32, clip: &MotionClip) -> f32 {
    let value = sample_motion_curve(curve, time, clip.restricted_beziers);
    if clip.looping
        && time < clip.duration + 1.0 / clip.fps
        && time
            > track
                .segments
                .last()
                .map_or(track.initial.time, |segment| segment.end().time)
    {
        let end = track
            .segments
            .last()
            .map_or(track.initial, MotionSegment::end);
        let cycle = clip.duration + 1.0 / clip.fps;
        return match track.segments.last() {
            Some(MotionSegment::Linear { .. } | MotionSegment::Bezier { .. }) => lerp(
                end.value,
                track.initial.value,
                ((time - end.time) / (cycle - end.time)).clamp(0.0, 1.0),
            ),
            Some(MotionSegment::InverseStepped { .. }) => track.initial.value,
            _ => value,
        };
    }
    value
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
fn lerp(a: f32, b: f32, weight: f32) -> f32 {
    a + (b - a) * weight
}
