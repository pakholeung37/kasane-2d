use kasane_core::types::Status;

use crate::schema::section;

use super::context::Moc3DecoderContext;
use super::helpers::read_i32;

impl<'a> Moc3DecoderContext<'a> {
    pub(super) fn decode_drawing_groups(&mut self) -> Result<(), Status> {
        let counts = &self.inspection.counts;
        let offsets = &self.inspection.section_offsets;
        let bytes = self.bytes;

        // Drawing hierarchy and tie order are independent of organization Parts.
        let mut groups = Vec::new();
        let mut owners = vec![None; counts.draw_groups as usize];
        if owners.is_empty() {
            return Err(Status::error(
                "INVALID_DRAW_GROUP",
                "Missing root drawing group",
            ));
        }
        owners[0] = Some(String::new());
        for index in 0..counts.draw_items as usize {
            if read_i32(
                bytes,
                offsets[section::DRAW_GROUP_OBJ_SRC_TYPE] as usize + index * 4,
            )? == 1
            {
                let part = read_i32(
                    bytes,
                    offsets[section::DRAW_GROUP_OBJ_SRC_IDX] as usize + index * 4,
                )?;
                let child = read_i32(
                    bytes,
                    offsets[section::DRAW_GROUP_OBJ_SRC_SELF_GROUP_IDX] as usize + index * 4,
                )?;
                if part < 0
                    || part as usize >= self.mapping.part_by_index.len()
                    || child <= 0
                    || child as usize >= owners.len()
                    || owners[child as usize].is_some()
                {
                    return Err(Status::error(
                        "INVALID_DRAW_GROUP",
                        format!("Invalid child group at drawing item {index}"),
                    ));
                }
                owners[child as usize] = Some(self.mapping.part_by_index[part as usize].clone());
            }
        }
        for (index, owner) in owners.into_iter().enumerate() {
            let owner = owner.ok_or_else(|| {
                Status::error(
                    "INVALID_DRAW_GROUP",
                    format!("Detached drawing group {index}"),
                )
            })?;
            let start = read_i32(
                bytes,
                offsets[section::DRAW_GROUP_SRC_OBJ_OFF] as usize + index * 4,
            )?;
            let length = read_i32(
                bytes,
                offsets[section::DRAW_GROUP_SRC_OBJ_LEN] as usize + index * 4,
            )?;
            if start < 0 || length < 0 || start as i64 + length as i64 > counts.draw_items as i64 {
                return Err(Status::error(
                    "INVALID_DRAW_GROUP",
                    format!("Invalid item range in group {index}"),
                ));
            }
            let mut items = Vec::new();
            for item in start as usize..(start + length) as usize {
                let kind = read_i32(
                    bytes,
                    offsets[section::DRAW_GROUP_OBJ_SRC_TYPE] as usize + item * 4,
                )?;
                let object = read_i32(
                    bytes,
                    offsets[section::DRAW_GROUP_OBJ_SRC_IDX] as usize + item * 4,
                )?;
                let table = match kind {
                    0 => &self.mapping.mesh_by_index,
                    1 => &self.mapping.part_by_index,
                    _ => {
                        return Err(Status::error(
                            "INVALID_DRAW_GROUP",
                            format!("Unknown item type {kind}"),
                        ))
                    }
                };
                let id = table.get(object as usize).ok_or_else(|| {
                    Status::error(
                        "INVALID_DRAW_GROUP",
                        format!("Invalid object at item {item}"),
                    )
                })?;
                items.push(id.clone());
            }
            groups.push(kasane_core::draw_order::DrawOrderGroup {
                owner,
                items,
                min_order: read_i32(
                    bytes,
                    offsets[section::DRAW_GROUP_SRC_MIN_ORDER] as usize + index * 4,
                )?,
                max_order: read_i32(
                    bytes,
                    offsets[section::DRAW_GROUP_SRC_MAX_ORDER] as usize + index * 4,
                )?,
            });
        }
        check_status!(self.doc.replace_draw_order_groups(groups).status);

        Ok(())
    }
}
