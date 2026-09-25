//! Framework-compatible fixed-step physics state for detached previews.
use std::collections::BTreeMap;

use crate::AnimationError;
use kasane_core::physics::{
    PhysicsAxis, PhysicsInput, PhysicsOutput, PhysicsRange, PhysicsSetting,
};
use kasane_core::{document::PhysicsAsset, Document};

#[derive(Clone, Copy, Default)]
struct Vector {
    x: f32,
    y: f32,
}
impl Vector {
    fn add(self, rhs: Self) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
    fn sub(self, rhs: Self) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
    fn mul(self, scalar: f32) -> Self {
        Self {
            x: self.x * scalar,
            y: self.y * scalar,
        }
    }
    fn normalized(self) -> Self {
        let length = (self.x * self.x + self.y * self.y).sqrt();
        if length == 0.0 {
            Self::default()
        } else {
            self.mul(1.0 / length)
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Particle {
    position: Vector,
    last_position: Vector,
    velocity: Vector,
    last_gravity: Vector,
}

struct RigState {
    particles: Vec<Particle>,
    current: Vec<f32>,
    previous: Vec<f32>,
}

pub(crate) struct PhysicsRuntime {
    remaining: f32,
    initialized_inputs: bool,
    parameter_cache: BTreeMap<String, f32>,
    input_cache: BTreeMap<String, f32>,
    rigs: Vec<RigState>,
}

/// Detached Physics-only state for rig editing and reference traces.
pub struct PhysicsPreview {
    document: Document,
    state: PhysicsRuntime,
    parameters: BTreeMap<String, f32>,
    diagnostics: Vec<String>,
}

impl PhysicsPreview {
    pub fn new(document: &Document) -> Self {
        let parameters = document
            .parameter_order()
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    document.get_parameter(id).expect("parameter").default_value,
                )
            })
            .collect();
        Self {
            document: document.clone(),
            state: PhysicsRuntime::new(document),
            parameters,
            diagnostics: Vec::new(),
        }
    }
    pub fn parameters(&self) -> &BTreeMap<String, f32> {
        &self.parameters
    }
    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }
    pub fn document_revision(&self) -> u64 {
        self.document.revision()
    }
    pub fn set_parameter(&mut self, id: &str, value: f32) -> Result<(), AnimationError> {
        let parameter = self
            .document
            .get_parameter(id)
            .ok_or_else(|| AnimationError::Evaluation(format!("missing parameter {id}")))?;
        if !value.is_finite() {
            return Err(AnimationError::InvalidTime);
        }
        self.parameters
            .insert(id.into(), value.clamp(parameter.minimum, parameter.maximum));
        Ok(())
    }
    pub fn advance(&mut self, dt: f32) -> Result<&BTreeMap<String, f32>, AnimationError> {
        if !dt.is_finite() || dt < 0.0 {
            return Err(AnimationError::InvalidTime);
        }
        self.diagnostics.clear();
        self.state.evaluate(
            &self.document,
            &mut self.parameters,
            dt,
            &mut self.diagnostics,
        );
        Ok(&self.parameters)
    }
    pub fn stabilize(&mut self) -> &BTreeMap<String, f32> {
        self.state.stabilize(&self.document, &mut self.parameters);
        &self.parameters
    }
    pub fn reset(&mut self) {
        self.state.reset(&self.document);
        self.parameters = self
            .document
            .parameter_order()
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    self.document
                        .get_parameter(id)
                        .expect("parameter")
                        .default_value,
                )
            })
            .collect();
        self.diagnostics.clear();
    }
}

impl PhysicsRuntime {
    pub(crate) fn new(document: &Document) -> Self {
        let parameter_cache = document
            .parameter_order()
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    document.get_parameter(id).expect("parameter").default_value,
                )
            })
            .collect::<BTreeMap<_, _>>();
        let rigs = document
            .physics()
            .map(|asset| {
                asset
                    .data
                    .settings
                    .iter()
                    .map(|setting| {
                        let mut position = Vector::default();
                        let particles = setting
                            .vertices
                            .iter()
                            .enumerate()
                            .map(|(index, vertex)| {
                                if index > 0 {
                                    position.y += vertex.radius;
                                }
                                Particle {
                                    position,
                                    last_position: position,
                                    velocity: Vector::default(),
                                    last_gravity: Vector { x: 0.0, y: 1.0 },
                                }
                            })
                            .collect();
                        RigState {
                            particles,
                            current: vec![0.0; setting.outputs.len()],
                            previous: vec![0.0; setting.outputs.len()],
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self {
            remaining: 0.0,
            initialized_inputs: false,
            input_cache: parameter_cache.clone(),
            parameter_cache,
            rigs,
        }
    }

    pub(crate) fn reset(&mut self, document: &Document) {
        *self = Self::new(document);
    }

    pub(crate) fn evaluate(
        &mut self,
        document: &Document,
        parameters: &mut BTreeMap<String, f32>,
        dt: f32,
        diagnostics: &mut Vec<String>,
    ) {
        let Some(asset) = document.physics() else {
            return;
        };
        for runtime in asset.referenced_runtime_ids() {
            if !asset.parameter_bindings.contains_key(runtime) {
                diagnostics.push(format!("Physics: unresolved parameter {runtime}"));
                return;
            }
        }
        if dt <= 0.0 {
            return;
        }
        if !self.initialized_inputs {
            self.input_cache = parameters.clone();
            self.initialized_inputs = true;
        }
        self.remaining += dt;
        if self.remaining > 5.0 {
            self.remaining = 0.0;
        }
        let step = asset
            .data
            .meta
            .fps
            .filter(|fps| *fps > 0.0)
            .map_or(dt, |fps| 1.0 / fps);
        // The official routine performs one or more fixed steps followed by
        // an interpolation of previous/current rig outputs.
        let mut count = 0;
        while self.remaining >= step && count < 100_000 {
            count += 1;
            let weight = step / self.remaining;
            for (id, value) in parameters.iter() {
                let previous = self.input_cache.get(id).copied().unwrap_or(*value);
                let blended = previous * (1.0 - weight) + *value * weight;
                self.parameter_cache.insert(id.clone(), blended);
                self.input_cache.insert(id.clone(), blended);
            }
            for (setting, rig) in asset.data.settings.iter().zip(&mut self.rigs) {
                rig.previous.clone_from(&rig.current);
                let (translation, angle) =
                    read_inputs(document, asset, setting, &self.parameter_cache);
                let radian = (-angle).to_radians();
                // Preserve Framework's in-place X update in the rotation.
                let x = translation.x * radian.cos() - translation.y * radian.sin();
                let y = x * radian.sin() + translation.y * radian.cos();
                update_particles(setting, rig, Vector { x, y }, angle, step);
                for (index, output) in setting.outputs.iter().enumerate() {
                    if output.vertex_index == 0 {
                        continue;
                    }
                    let value = output_value(output, &rig.particles);
                    rig.current[index] = value;
                    write_output(document, asset, output, value, &mut self.parameter_cache);
                }
            }
            self.remaining -= step;
        }
        if count >= 100_000 {
            diagnostics.push("Physics: step limit reached".into());
        }
        let alpha = self.remaining / step;
        for (setting, rig) in asset.data.settings.iter().zip(&self.rigs) {
            for (index, output) in setting.outputs.iter().enumerate() {
                if output.vertex_index == 0 {
                    continue;
                }
                let value = rig.previous[index] * (1.0 - alpha) + rig.current[index] * alpha;
                write_output(document, asset, output, value, parameters);
            }
        }
    }

    pub(crate) fn stabilize(
        &mut self,
        document: &Document,
        parameters: &mut BTreeMap<String, f32>,
    ) {
        let Some(asset) = document.physics() else {
            return;
        };
        self.parameter_cache = parameters.clone();
        self.input_cache = parameters.clone();
        self.initialized_inputs = true;
        for (setting, rig) in asset.data.settings.iter().zip(&mut self.rigs) {
            let (translation, angle) = read_inputs(document, asset, setting, parameters);
            let radian = (-angle).to_radians();
            let x = translation.x * radian.cos() - translation.y * radian.sin();
            let y = x * radian.sin() + translation.y * radian.cos();
            rig.particles[0].position = Vector { x, y };
            let gravity = Vector {
                x: angle.to_radians().sin(),
                y: angle.to_radians().cos(),
            }
            .normalized();
            for index in 1..rig.particles.len() {
                let vertex = &setting.vertices[index];
                let force = gravity
                    .mul(vertex.acceleration)
                    .normalized()
                    .mul(vertex.radius);
                let mut position = rig.particles[index - 1].position.add(force);
                if position.x.abs() < 0.001 * setting.normalization.position.maximum {
                    position.x = 0.0;
                }
                rig.particles[index].last_position = rig.particles[index].position;
                rig.particles[index].position = position;
                rig.particles[index].velocity = Vector::default();
                rig.particles[index].last_gravity = gravity;
            }
            for (index, output) in setting.outputs.iter().enumerate() {
                if output.vertex_index == 0 {
                    continue;
                }
                let value = output_value(output, &rig.particles);
                rig.current[index] = value;
                rig.previous[index] = value;
                write_output(document, asset, output, value, parameters);
            }
        }
    }
}

fn read_inputs(
    document: &Document,
    asset: &PhysicsAsset,
    setting: &PhysicsSetting,
    values: &BTreeMap<String, f32>,
) -> (Vector, f32) {
    let mut translation = Vector::default();
    let mut angle = 0.0;
    for input in &setting.inputs {
        let Some(id) = asset.parameter_bindings.get(&input.source.id) else {
            continue;
        };
        let Some(parameter) = document.get_parameter(id) else {
            continue;
        };
        let value = values.get(id).copied().unwrap_or(parameter.default_value);
        let range = if input.kind == PhysicsAxis::Angle {
            setting.normalization.angle
        } else {
            setting.normalization.position
        };
        let normalized = normalize(value, parameter.minimum, parameter.maximum, range, input);
        match input.kind {
            PhysicsAxis::X => translation.x += normalized,
            PhysicsAxis::Y => translation.y += normalized,
            PhysicsAxis::Angle => angle += normalized,
        }
    }
    (translation, angle)
}

fn normalize(
    value: f32,
    minimum: f32,
    maximum: f32,
    range: PhysicsRange,
    input: &PhysicsInput,
) -> f32 {
    let value = value.clamp(minimum, maximum);
    let middle = minimum + (maximum - minimum) / 2.0;
    let deviation = value - middle;
    let result = if deviation > 0.0 && maximum != middle {
        deviation * ((range.maximum - range.default) / (maximum - middle)) + range.default
    } else if deviation < 0.0 && minimum != middle {
        deviation * ((range.minimum - range.default) / (minimum - middle)) + range.default
    } else {
        range.default
    };
    result * (if input.reflect { 1.0 } else { -1.0 }) * (input.weight / 100.0)
}

fn update_particles(
    setting: &PhysicsSetting,
    rig: &mut RigState,
    translation: Vector,
    angle: f32,
    step: f32,
) {
    rig.particles[0].position = translation;
    let gravity = Vector {
        x: angle.to_radians().sin(),
        y: angle.to_radians().cos(),
    }
    .normalized();
    for index in 1..rig.particles.len() {
        let vertex = &setting.vertices[index];
        let previous = rig.particles[index - 1].position;
        let particle = &mut rig.particles[index];
        particle.last_position = particle.position;
        let delay = vertex.delay * step * 30.0;
        let mut direction = particle.position.sub(previous);
        let radian = direction_radian(particle.last_gravity, gravity) / 5.0;
        direction.x = radian.cos() * direction.x - direction.y * radian.sin();
        direction.y = radian.sin() * direction.x + direction.y * radian.cos();
        let force = gravity.mul(vertex.acceleration);
        particle.position = previous
            .add(direction)
            .add(particle.velocity.mul(delay))
            .add(force.mul(delay * delay));
        particle.position = previous.add(
            particle
                .position
                .sub(previous)
                .normalized()
                .mul(vertex.radius),
        );
        if particle.position.x.abs() < 0.001 * setting.normalization.position.maximum {
            particle.position.x = 0.0;
        }
        if delay != 0.0 {
            particle.velocity = particle
                .position
                .sub(particle.last_position)
                .mul(vertex.mobility / delay);
        }
        particle.last_gravity = gravity;
    }
}

fn direction_radian(from: Vector, to: Vector) -> f32 {
    let mut result = to.y.atan2(to.x) - from.y.atan2(from.x);
    while result < -std::f32::consts::PI {
        result += std::f32::consts::TAU;
    }
    while result > std::f32::consts::PI {
        result -= std::f32::consts::TAU;
    }
    result
}

fn output_value(output: &PhysicsOutput, particles: &[Particle]) -> f32 {
    let index = output.vertex_index;
    let translation = particles[index].position.sub(particles[index - 1].position);
    let value = match output.kind {
        PhysicsAxis::X | PhysicsAxis::Y => 0.0, // Framework TranslationScale is zero-initialized.
        PhysicsAxis::Angle => {
            let parent = if index >= 2 {
                particles[index - 1]
                    .position
                    .sub(particles[index - 2].position)
            } else {
                Vector { x: 0.0, y: 1.0 }
            };
            direction_radian(parent, translation)
        }
    };
    if output.reflect {
        -value
    } else {
        value
    }
}

fn write_output(
    document: &Document,
    asset: &PhysicsAsset,
    output: &PhysicsOutput,
    raw: f32,
    values: &mut BTreeMap<String, f32>,
) {
    let Some(id) = asset.parameter_bindings.get(&output.destination.id) else {
        return;
    };
    let Some(parameter) = document.get_parameter(id) else {
        return;
    };
    let value = (raw * output.scale).clamp(parameter.minimum, parameter.maximum);
    let current = values.get(id).copied().unwrap_or(parameter.default_value);
    let weight = output.weight / 100.0;
    values.insert(
        id.clone(),
        if weight >= 1.0 {
            value
        } else {
            current * (1.0 - weight) + value * weight
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use kasane_core::{Canvas, Parameter, Vec2};
    use kasane_project::import_physics3;

    const DOC: &str = "00000000-0000-4000-8000-000000000f01";
    const X: &str = "00000000-0000-4000-8000-000000000f02";
    const Y: &str = "00000000-0000-4000-8000-000000000f03";
    const PHYSICS: &str = "00000000-0000-4000-8000-000000000f04";

    fn document(source: &str) -> Document {
        let mut doc = Document::new();
        assert!(doc
            .initialize(DOC, Canvas::new(100.0, 100.0, Vec2::default(), 10.0))
            .is_ok());
        for (id, runtime) in [(X, "ParamX"), (Y, "ParamY")] {
            assert!(doc
                .create_parameter(Parameter {
                    id: id.into(),
                    runtime_id: runtime.into(),
                    name: runtime.into(),
                    minimum: -1.0,
                    maximum: 1.0,
                    default_value: 0.0,
                    ..Default::default()
                })
                .status
                .is_ok());
        }
        import_physics3(&doc, PHYSICS, source).unwrap().candidate
    }

    #[test]
    #[allow(clippy::excessive_precision)] // Expected values are copied from the official trace.
    fn official_physics_fps_traces() {
        let cases: [(&str, [f32; 6]); 4] = [
            (
                include_str!("../../../tests/fixtures/animation_cpu/missing.physics3.json"),
                [
                    0.0,
                    0.0,
                    -0.654304147,
                    -0.307027221,
                    0.951560616,
                    -0.46348238,
                ],
            ),
            (
                include_str!("../../../tests/fixtures/animation_cpu/zero.physics3.json"),
                [
                    0.0,
                    0.0,
                    -0.654304147,
                    -0.307027221,
                    0.951560616,
                    -0.46348238,
                ],
            ),
            (
                include_str!("../../../tests/fixtures/animation_cpu/thirty.physics3.json"),
                [
                    0.0,
                    0.0,
                    -0.212036729,
                    -0.424073458,
                    0.786318183,
                    -0.315223336,
                ],
            ),
            (
                include_str!("../../../tests/fixtures/animation_cpu/multi.physics3.json"),
                [
                    0.0,
                    0.0,
                    -0.0833962262,
                    -0.166792452,
                    0.311171353,
                    -0.122322865,
                ],
            ),
        ];
        for (source, expected) in cases {
            let doc = document(source);
            let mut physics = PhysicsRuntime::new(&doc);
            for (index, (dt, input)) in [
                (1.0 / 60.0, 0.0),
                (1.0 / 60.0, 1.0),
                (1.0 / 60.0, 1.0),
                (1.0 / 60.0, -1.0),
                (1.0 / 30.0, -1.0),
                (0.1, 0.0),
            ]
            .into_iter()
            .enumerate()
            {
                let mut values = BTreeMap::from([(X.into(), input), (Y.into(), 0.0)]);
                physics.evaluate(&doc, &mut values, dt, &mut Vec::new());
                assert!(
                    (values[Y] - expected[index]).abs() < 0.0001,
                    "fps={:?} frame={index} actual={} expected={}",
                    doc.physics().unwrap().data.meta.fps,
                    values[Y],
                    expected[index]
                );
            }
        }
    }
}
