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
            let mut max = read_f32(
                self.bytes,
                offsets[section::PARAM_SRC_MAXIMUM_VALUE] as usize + p * 4,
            )?;
            let mut min = read_f32(
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

            // Real MOC3 files can declare a narrower slider range than their
            // authored keyforms. Include those keys so no source keyform is
            // clamped or discarded by the Document's range invariant.
            for (param_off_section, param_len_section, key_off_section, key_len_section) in [
                (
                    section::PARAM_SRC_KEY_TABLE_OFF,
                    section::PARAM_SRC_KEY_TABLE_LEN,
                    section::KEY_TABLE_SRC_KEYS_OFF,
                    section::KEY_TABLE_SRC_KEYS_LEN,
                ),
                (
                    section::PARAM_SRC_BLEND_KEY_TABLE_OFF,
                    section::PARAM_SRC_BLEND_KEY_TABLE_LEN,
                    section::BLEND_KEY_TABLE_SRC_KEYS_OFF,
                    section::BLEND_KEY_TABLE_SRC_KEYS_LEN,
                ),
            ] {
                if offsets.len() <= key_len_section || offsets[param_off_section] == 0 {
                    continue;
                }
                let table_off = read_i32(self.bytes, offsets[param_off_section] as usize + p * 4)?;
                let table_len = read_i32(self.bytes, offsets[param_len_section] as usize + p * 4)?;
                if table_len == 0 {
                    continue;
                }
                if table_off < 0 || table_len < 0 {
                    return Err(Status::error(
                        "INVALID_KEY_TABLE",
                        format!("Parameter {p}: invalid key-table range"),
                    ));
                }
                for table in table_off as usize..table_off as usize + table_len as usize {
                    let keys_off =
                        read_i32(self.bytes, offsets[key_off_section] as usize + table * 4)?;
                    let keys_len =
                        read_i32(self.bytes, offsets[key_len_section] as usize + table * 4)?;
                    if keys_off < 0 || keys_len < 0 {
                        return Err(Status::error(
                            "INVALID_KEYS",
                            format!("Parameter {p}: invalid key range"),
                        ));
                    }
                    for key in keys_off as usize..keys_off as usize + keys_len as usize {
                        let value = read_f32(
                            self.bytes,
                            offsets[section::KEYS_SRC_KEY] as usize + key * 4,
                        )?;
                        if value.is_finite() {
                            min = min.min(value);
                            max = max.max(value);
                        }
                    }
                }
            }

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
