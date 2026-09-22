use kasane_core::types::{Parameter, ParameterKind, Status};

use crate::schema::section;

use super::context::Moc3DecoderContext;
use super::helpers::{read_f32, read_i32, read_string};

impl<'a> Moc3DecoderContext<'a> {
    pub(super) fn decode_parameters(&mut self) -> Result<(), Status> {
        let counts = &self.inspection.counts;
        let offsets = &self.inspection.section_offsets;
        let ver = self.inspection.version_number;

        for p in 0..counts.parameters as usize {
            let id = self.mapping.parameter_by_index[p].clone();
            let runtime_id = read_string(
                self.bytes,
                offsets[section::PARAM_SRC_ID] as usize + p * 64,
                64,
            );
            let max = read_f32(
                self.bytes,
                offsets[section::PARAM_SRC_MAXIMUM_VALUE] as usize + p * 4,
            )?;
            let min = read_f32(
                self.bytes,
                offsets[section::PARAM_SRC_MINIMUM_VALUE] as usize + p * 4,
            )?;
            let default_val = read_f32(
                self.bytes,
                offsets[section::PARAM_SRC_DEFAULT_VALUE] as usize + p * 4,
            )?;
            let dec_places = read_i32(
                self.bytes,
                offsets[section::PARAM_SRC_DECIMAL_PLACES] as usize + p * 4,
            )?;
            let repeat = if offsets.len() > 54 && offsets[section::PARAM_SRC_REPEAT] > 0 {
                read_i32(
                    self.bytes,
                    offsets[section::PARAM_SRC_REPEAT] as usize + p * 4,
                )? != 0
            } else {
                false
            };
            let param_type =
                if ver >= 4 && offsets.len() > 114 && offsets[section::PARAM_SRC_TYPE] > 0 {
                    read_i32(
                        self.bytes,
                        offsets[section::PARAM_SRC_TYPE] as usize + p * 4,
                    )?
                } else {
                    0
                };
            let kind = match param_type {
                0 => ParameterKind::Normal,
                1 => ParameterKind::BlendShape,
                _ => {
                    return Err(Status::error(
                        "UNSUPPORTED_FEATURE",
                        format!("Parameter {p}: unknown type {param_type}"),
                    ))
                }
            };

            check_status!(
                self.doc
                    .create_parameter(Parameter {
                        id,
                        runtime_id: if runtime_id.is_empty() {
                            format!("Param{p}")
                        } else {
                            runtime_id.clone()
                        },
                        name: if runtime_id.is_empty() {
                            format!("Param{p}")
                        } else {
                            runtime_id
                        },
                        minimum: min,
                        maximum: max,
                        default_value: default_val,
                        decimal_places: dec_places,
                        kind,
                        repeat,
                    })
                    .status
            );
        }
        Ok(())
    }
}
