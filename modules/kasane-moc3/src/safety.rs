//! Rust-only table and reference checks. Runs before optional native validation.
use crate::schema::{section::*, VersionLayout, SCHEMA};
use kasane_core::Status;

pub(crate) struct Tables<'a> {
    bytes: &'a [u8],
    offsets: &'a [u32],
    counts: &'a [i32],
    layout: VersionLayout,
}
impl<'a> Tables<'a> {
    fn count(&self, field: usize) -> usize {
        let c = SCHEMA[field].count_index;
        if c < 0 {
            1
        } else {
            self.counts.get(c as usize).copied().unwrap_or(0) as usize
        }
    }
    fn int(&self, field: usize, row: usize) -> Result<i32, Status> {
        let schema = self.layout.require(field)?;
        if row >= self.count(field) {
            return Err(Status::error("INDEX_OUT_OF_BOUNDS", schema.name));
        }
        let start = self.offsets[field] as usize + row * schema.width;
        let bytes = self
            .bytes
            .get(start..start + 4)
            .ok_or_else(|| Status::error("TRUNCATED_BUFFER", schema.name))?;
        Ok(i32::from_le_bytes(bytes.try_into().unwrap()))
    }
    fn window(&self, start: i32, len: i32, total: usize, field: usize) -> Result<(), Status> {
        // Empty windows may have a negative sentinel; nothing is read.
        if len < 0
            || (len > 0
                && (start < 0 || start as usize > total || len as usize > total - start as usize))
        {
            return Err(Status::error("INDEX_OUT_OF_BOUNDS", SCHEMA[field].name));
        }
        Ok(())
    }
}

pub(crate) fn validate(
    bytes: &[u8],
    offsets: &[u32],
    counts: &[i32],
    layout: VersionLayout,
) -> Result<(), Status> {
    // Reserved counts cannot drive allocations for sections absent in this version.
    // Counts above the last modeled section remain reserved and are not consumed.
    for (index, &count) in counts.iter().enumerate().take(38) {
        let supported = SCHEMA
            .iter()
            .any(|s| s.count_index == index as i32 && s.minimum_version <= layout.version);
        if !supported && count != 0 {
            return Err(Status::error(
                "UNSUPPORTED_FIELD",
                format!(
                    "count_info[{index}] is unavailable in version {}",
                    layout.version
                ),
            ));
        }
    }
    let t = Tables {
        bytes,
        offsets,
        counts,
        layout,
    };
    // (reference field, target count, allow -1)
    const REFERENCES: &[(usize, usize, bool)] = &[
        (PART_SRC_BINDING_IDX, 12, false),
        (PART_SRC_PARENT_PART_IDX, 0, true),
        (DEFORMER_SRC_BINDING_IDX, 12, false),
        (DEFORMER_SRC_PARENT_PART_IDX, 0, true),
        (DEFORMER_SRC_PARENT_DEFORMER_IDX, 1, true),
        (WARP_SRC_BINDING_IDX, 12, false),
        (ROTATION_SRC_BINDING_IDX, 12, false),
        (ART_MESH_SRC_BINDING_IDX, 12, false),
        (ART_MESH_SRC_PARENT_PART_IDX, 0, true),
        (ART_MESH_SRC_PARENT_DEFORMER_IDX, 1, true),
        (KEY_TABLE_IDX_SRC_IDX, 13, false),
        (MASK_SRC_ART_MESH_IDX, 4, true),
        (GLUE_SRC_BINDING_IDX, 12, false),
        (GLUE_SRC_ART_MESH_IDX_A, 4, false),
        (GLUE_SRC_ART_MESH_IDX_B, 4, false),
        (DRAW_GROUP_OBJ_SRC_SELF_GROUP_IDX, 18, true),
        (BLEND_BINDING_SRC_KEY_TABLE_IDX, 25, false),
        (BS_WARP_SRC_TARGET_IDX, 2, false),
        (BS_ART_MESH_SRC_TARGET_IDX, 4, false),
        (BLEND_CONSTRAINT_IDX_SRC_CONSTRAINT_IDX, 30, false),
        (BLEND_CONSTRAINT_SRC_PARAMETER_IDX, 5, false),
        (BS_PART_SRC_TARGET_IDX, 0, false),
        (BS_ROTATION_SRC_TARGET_IDX, 3, false),
        (BS_GLUE_SRC_TARGET_IDX, 20, false),
        (PART_SRC_OFFSCREEN_IDX, 35, true),
        (OFFSCREEN_SRC_OWNER_IDX, 0, false),
        (BS_OFFSCREEN_SRC_TARGET_IDX, 35, false),
        (PART_KEY_SRC_KEY_IDX, 36, true),
        (OFFSCREEN_KEY_SRC_KEY_MUL_COLOR_OFF, 23, true),
        (OFFSCREEN_KEY_SRC_KEY_SCR_COLOR_OFF, 24, true),
    ];
    for &(field, target, sentinel) in REFERENCES {
        if SCHEMA[field].minimum_version > layout.version {
            continue;
        }
        for row in 0..t.count(field) {
            let value = t.int(field, row)?;
            if !(sentinel && value == -1)
                && (value < 0 || value as usize >= counts[target] as usize)
            {
                return Err(Status::error(
                    "INDEX_OUT_OF_BOUNDS",
                    format!("{}[{row}]={value}", SCHEMA[field].name),
                ));
            }
        }
    }
    // (window start, window length, target count)
    const WINDOWS: &[(usize, usize, usize)] = &[
        (PART_SRC_KEYFORM_OFF, PART_SRC_KEY_LEN, 6),
        (WARP_SRC_KEYFORM_OFF, WARP_SRC_KEY_LEN, 7),
        (ROTATION_SRC_KEYFORM_OFF, ROTATION_SRC_KEY_LEN, 8),
        (ART_MESH_SRC_KEYFORM_OFF, ART_MESH_SRC_KEY_LEN, 9),
        (ART_MESH_SRC_IDX_OFF, ART_MESH_SRC_IDX_LEN, 16),
        (ART_MESH_SRC_MASK_OFF, ART_MESH_SRC_MASK_LEN, 17),
        (PARAM_SRC_KEY_TABLE_OFF, PARAM_SRC_KEY_TABLE_LEN, 13),
        (
            BINDING_SRC_KEY_TABLE_IDX_OFF,
            BINDING_SRC_KEY_TABLE_IDX_LEN,
            11,
        ),
        (KEY_TABLE_SRC_KEYS_OFF, KEY_TABLE_SRC_KEYS_LEN, 14),
        (GLUE_SRC_KEYFORM_OFF, GLUE_SRC_KEY_LEN, 22),
        (GLUE_SRC_INFO_OFF, GLUE_SRC_INFO_LEN, 21),
        (DRAW_GROUP_SRC_OBJ_OFF, DRAW_GROUP_SRC_OBJ_LEN, 19),
        (WARP_SRC_KEY_COLOR_OFF, WARP_SRC_KEY_LEN, 23),
        (ROTATION_SRC_KEY_COLOR_OFF, ROTATION_SRC_KEY_LEN, 23),
        (ART_MESH_SRC_KEY_COLOR_OFF, ART_MESH_SRC_KEY_LEN, 23),
        (PARAM_KEYS_SRC_KEYS_OFF, PARAM_KEYS_SRC_KEYS_LEN, 14),
        (
            BLEND_KEY_TABLE_SRC_KEYS_OFF,
            BLEND_KEY_TABLE_SRC_KEYS_LEN,
            14,
        ),
        (
            PARAM_SRC_BLEND_KEY_TABLE_OFF,
            PARAM_SRC_BLEND_KEY_TABLE_LEN,
            25,
        ),
        (
            BLEND_BINDING_SRC_BS_CONSTRAINT_IDX_OFF,
            BLEND_BINDING_SRC_BS_CONSTRAINT_IDX_LEN,
            29,
        ),
        (BS_WARP_SRC_BS_BINDING_OFF, BS_WARP_SRC_BS_BINDING_LEN, 26),
        (
            BS_ART_MESH_SRC_BS_BINDING_OFF,
            BS_ART_MESH_SRC_BS_BINDING_LEN,
            26,
        ),
        (
            BLEND_CONSTRAINT_SRC_VALUE_OFF,
            BLEND_CONSTRAINT_SRC_VALUE_LEN,
            31,
        ),
        (BS_PART_SRC_BS_BINDING_OFF, BS_PART_SRC_BS_BINDING_LEN, 26),
        (
            BS_ROTATION_SRC_BS_BINDING_OFF,
            BS_ROTATION_SRC_BS_BINDING_LEN,
            26,
        ),
        (BS_GLUE_SRC_BS_BINDING_OFF, BS_GLUE_SRC_BS_BINDING_LEN, 26),
        (OFFSCREEN_SRC_MASK_OFF, OFFSCREEN_SRC_MASK_LEN, 17),
        (
            BS_OFFSCREEN_SRC_BS_BINDING_OFF,
            BS_OFFSCREEN_SRC_BS_BINDING_LEN,
            26,
        ),
    ];
    for &(start, length, target) in WINDOWS {
        if SCHEMA[start].minimum_version > layout.version {
            continue;
        }
        for row in 0..t.count(start) {
            let offset = t.int(start, row)?;
            // Missing ordinary colors are represented by negative offsets.
            if SCHEMA[start].name.ends_with("key_color_off") && offset < 0 {
                continue;
            }
            t.window(offset, t.int(length, row)?, counts[target] as usize, start)?;
        }
    }
    for row in 0..t.count(DEFORMER_SRC_TYPE) {
        let kind = t.int(DEFORMER_SRC_TYPE, row)?;
        let local = t.int(DEFORMER_SRC_LOCAL_IDX, row)?;
        let total = match kind {
            0 => counts[2],
            1 => counts[3],
            _ => return Err(Status::error("INVALID_DEFORMER_TYPE", kind.to_string())),
        };
        if local < 0 || local >= total {
            return Err(Status::error("INDEX_OUT_OF_BOUNDS", "deformer.local_idx"));
        }
    }
    for row in 0..t.count(WARP_SRC_ROW) {
        let rows = t.int(WARP_SRC_ROW, row)? as i64;
        let cols = t.int(WARP_SRC_COL, row)? as i64;
        let vertices = t.int(WARP_SRC_VERTEX_COUNT, row)? as i64;
        if rows <= 0 || cols <= 0 || (rows + 1) * (cols + 1) != vertices {
            return Err(Status::error(
                "INVALID_WARP_GRID",
                "warp dimensions must match vertex count",
            ));
        }
    }
    for row in 0..t.count(BINDING_SRC_KEY_TABLE_IDX_LEN) {
        if t.int(BINDING_SRC_KEY_TABLE_IDX_LEN, row)? > 16 {
            return Err(Status::error("CAPACITY", "binding axes exceed 16"));
        }
    }
    // An in-bounds list of axes can still describe an enormous Cartesian grid.
    // Check the product before the decoder allocates or multiplies dimensions.
    let mut grids = Vec::with_capacity(counts[12] as usize);
    for binding in 0..counts[12] as usize {
        let start = t.int(BINDING_SRC_KEY_TABLE_IDX_OFF, binding)?;
        let length = t.int(BINDING_SRC_KEY_TABLE_IDX_LEN, binding)?;
        let mut grid = 1usize;
        for axis in 0..length {
            let table = t.int(KEY_TABLE_IDX_SRC_IDX, (start + axis) as usize)?;
            let keys = t.int(KEY_TABLE_SRC_KEYS_LEN, table as usize)?;
            if keys <= 0 {
                return Err(Status::error("INVALID_BINDING", "binding axis has no keys"));
            }
            grid = grid
                .checked_mul(keys as usize)
                .ok_or_else(|| Status::error("CAPACITY", "binding grid overflow"))?;
        }
        grids.push((length, grid));
    }
    for (binding, length) in [
        (PART_SRC_BINDING_IDX, PART_SRC_KEY_LEN),
        (WARP_SRC_BINDING_IDX, WARP_SRC_KEY_LEN),
        (ROTATION_SRC_BINDING_IDX, ROTATION_SRC_KEY_LEN),
        (ART_MESH_SRC_BINDING_IDX, ART_MESH_SRC_KEY_LEN),
        (GLUE_SRC_BINDING_IDX, GLUE_SRC_KEY_LEN),
    ] {
        for row in 0..t.count(binding) {
            let (axes, grid) = grids[t.int(binding, row)? as usize];
            if axes > 0 && grid > t.int(length, row)? as usize {
                return Err(Status::error(
                    "INDEX_OUT_OF_BOUNDS",
                    format!(
                        "{}: binding grid exceeds keyform window",
                        SCHEMA[length].name
                    ),
                ));
            }
        }
    }
    // Geometry spans must stay within their pools, not merely within the file.
    for (vertices, start, length, positions) in [
        (
            WARP_SRC_VERTEX_COUNT,
            WARP_SRC_KEYFORM_OFF,
            WARP_SRC_KEY_LEN,
            WARP_KEY_SRC_KEY_POS_OFF,
        ),
        (
            ART_MESH_SRC_VERTEX_COUNT,
            ART_MESH_SRC_KEYFORM_OFF,
            ART_MESH_SRC_KEY_LEN,
            ART_MESH_KEY_SRC_KEY_POS_OFF,
        ),
    ] {
        for row in 0..t.count(vertices) {
            let vc = t.int(vertices, row)?;
            let span = vc
                .checked_mul(2)
                .filter(|v| *v >= 0)
                .ok_or_else(|| Status::error("CAPACITY", "vertex span"))?;
            if vertices == ART_MESH_SRC_VERTEX_COUNT {
                t.window(
                    t.int(ART_MESH_SRC_UV_OFF, row)?,
                    span,
                    counts[15] as usize,
                    ART_MESH_SRC_UV_OFF,
                )?;
            }
            let off = t.int(start, row)?;
            for key in 0..t.int(length, row)? {
                t.window(
                    t.int(positions, (off + key) as usize)?,
                    span,
                    counts[10] as usize,
                    positions,
                )?;
            }
        }
    }
    Ok(())
}
