// Wire layout shared by inspection, decoding and encoding.
// Derived from PurismCore src/moc3.h (MIT), Copyright (c) 2026 Sakura Motion Project.
use kasane_core::Status;

#[derive(Debug, Clone, Copy)]
pub struct SectionSchema {
    pub name: &'static str,
    pub width: usize,
    pub count_index: i32,
    pub minimum_version: u8,
}
macro_rules! sections {
    ($(($id:ident, $index:expr, $name:expr, $width:expr, $count:expr, $version:expr)),* $(,)?) => {
        pub mod section { $(pub const $id: usize = $index;)* }
        pub const SCHEMA: &[SectionSchema] = &[$(SectionSchema {
            name: $name, width: $width, count_index: $count, minimum_version: $version,
        }),*];
    }
}
sections! {
    (COUNT_INFO, 0, "count_info", 256, -1, 1),
    (CANVAS_INFO, 1, "canvas_info", 24, -1, 1),
    (PART_SRC_ID_RUNTIME, 2, "part_src.id_runtime", 8, 0, 1),
    (PART_SRC_ID, 3, "part_src.id", 64, 0, 1),
    (PART_SRC_BINDING_IDX, 4, "part_src.binding_idx", 4, 0, 1),
    (PART_SRC_KEYFORM_OFF, 5, "part_src.keyform_off", 4, 0, 1),
    (PART_SRC_KEY_LEN, 6, "part_src.key_len", 4, 0, 1),
    (PART_SRC_VISIBLE, 7, "part_src.visible", 4, 0, 1),
    (PART_SRC_ENABLE, 8, "part_src.enable", 4, 0, 1),
    (PART_SRC_PARENT_PART_IDX, 9, "part_src.parent_part_idx", 4, 0, 1),
    (DEFORMER_SRC_ID_RUNTIME, 10, "deformer_src.id_runtime", 8, 1, 1),
    (DEFORMER_SRC_ID, 11, "deformer_src.id", 64, 1, 1),
    (DEFORMER_SRC_BINDING_IDX, 12, "deformer_src.binding_idx", 4, 1, 1),
    (DEFORMER_SRC_VISIBLE, 13, "deformer_src.visible", 4, 1, 1),
    (DEFORMER_SRC_ENABLE, 14, "deformer_src.enable", 4, 1, 1),
    (DEFORMER_SRC_PARENT_PART_IDX, 15, "deformer_src.parent_part_idx", 4, 1, 1),
    (DEFORMER_SRC_PARENT_DEFORMER_IDX, 16, "deformer_src.parent_deformer_idx", 4, 1, 1),
    (DEFORMER_SRC_TYPE, 17, "deformer_src.type", 4, 1, 1),
    (DEFORMER_SRC_LOCAL_IDX, 18, "deformer_src.local_idx", 4, 1, 1),
    (WARP_SRC_BINDING_IDX, 19, "warp_src.binding_idx", 4, 2, 1),
    (WARP_SRC_KEYFORM_OFF, 20, "warp_src.keyform_off", 4, 2, 1),
    (WARP_SRC_KEY_LEN, 21, "warp_src.key_len", 4, 2, 1),
    (WARP_SRC_VERTEX_COUNT, 22, "warp_src.vertex_count", 4, 2, 1),
    (WARP_SRC_ROW, 23, "warp_src.row", 4, 2, 1),
    (WARP_SRC_COL, 24, "warp_src.col", 4, 2, 1),
    (ROTATION_SRC_BINDING_IDX, 25, "rotation_src.binding_idx", 4, 3, 1),
    (ROTATION_SRC_KEYFORM_OFF, 26, "rotation_src.keyform_off", 4, 3, 1),
    (ROTATION_SRC_KEY_LEN, 27, "rotation_src.key_len", 4, 3, 1),
    (ROTATION_SRC_BASE_ANGLE, 28, "rotation_src.base_angle", 4, 3, 1),
    (ART_MESH_SRC_ID_RUNTIME, 29, "art_mesh_src.id_runtime", 8, 4, 1),
    (ART_MESH_SRC_UV_RUNTIME, 30, "art_mesh_src.uv_runtime", 8, 4, 1),
    (ART_MESH_SRC_POS_IDX_RUNTIME, 31, "art_mesh_src.pos_idx_runtime", 8, 4, 1),
    (ART_MESH_SRC_DRAWABLE_MASK_RUNTIME, 32, "art_mesh_src.drawable_mask_runtime", 8, 4, 1),
    (ART_MESH_SRC_ID, 33, "art_mesh_src.id", 64, 4, 1),
    (ART_MESH_SRC_BINDING_IDX, 34, "art_mesh_src.binding_idx", 4, 4, 1),
    (ART_MESH_SRC_KEYFORM_OFF, 35, "art_mesh_src.keyform_off", 4, 4, 1),
    (ART_MESH_SRC_KEY_LEN, 36, "art_mesh_src.key_len", 4, 4, 1),
    (ART_MESH_SRC_VISIBLE, 37, "art_mesh_src.visible", 4, 4, 1),
    (ART_MESH_SRC_ENABLE, 38, "art_mesh_src.enable", 4, 4, 1),
    (ART_MESH_SRC_PARENT_PART_IDX, 39, "art_mesh_src.parent_part_idx", 4, 4, 1),
    (ART_MESH_SRC_PARENT_DEFORMER_IDX, 40, "art_mesh_src.parent_deformer_idx", 4, 4, 1),
    (ART_MESH_SRC_TEXTURE_NO, 41, "art_mesh_src.texture_no", 4, 4, 1),
    (ART_MESH_SRC_DRAWABLE_FLAG, 42, "art_mesh_src.drawable_flag", 1, 4, 1),
    (ART_MESH_SRC_VERTEX_COUNT, 43, "art_mesh_src.vertex_count", 4, 4, 1),
    (ART_MESH_SRC_UV_OFF, 44, "art_mesh_src.uv_off", 4, 4, 1),
    (ART_MESH_SRC_IDX_OFF, 45, "art_mesh_src.idx_off", 4, 4, 1),
    (ART_MESH_SRC_IDX_LEN, 46, "art_mesh_src.idx_len", 4, 4, 1),
    (ART_MESH_SRC_MASK_OFF, 47, "art_mesh_src.mask_off", 4, 4, 1),
    (ART_MESH_SRC_MASK_LEN, 48, "art_mesh_src.mask_len", 4, 4, 1),
    (PARAM_SRC_ID_RUNTIME, 49, "param_src.id_runtime", 8, 5, 1),
    (PARAM_SRC_ID, 50, "param_src.id", 64, 5, 1),
    (PARAM_SRC_MAXIMUM_VALUE, 51, "param_src.maximum_value", 4, 5, 1),
    (PARAM_SRC_MINIMUM_VALUE, 52, "param_src.minimum_value", 4, 5, 1),
    (PARAM_SRC_DEFAULT_VALUE, 53, "param_src.default_value", 4, 5, 1),
    (PARAM_SRC_REPEAT, 54, "param_src.repeat", 4, 5, 1),
    (PARAM_SRC_DECIMAL_PLACES, 55, "param_src.decimal_places", 4, 5, 1),
    (PARAM_SRC_KEY_TABLE_OFF, 56, "param_src.key_table_off", 4, 5, 1),
    (PARAM_SRC_KEY_TABLE_LEN, 57, "param_src.key_table_len", 4, 5, 1),
    (PART_KEY_SRC_DRAW_ORDER, 58, "part_key_src.draw_order", 4, 6, 1),
    (WARP_KEY_SRC_OPACITY, 59, "warp_key_src.opacity", 4, 7, 1),
    (WARP_KEY_SRC_KEY_POS_OFF, 60, "warp_key_src.key_pos_off", 4, 7, 1),
    (ROTATION_KEY_SRC_OPACITY, 61, "rotation_key_src.opacity", 4, 8, 1),
    (ROTATION_KEY_SRC_ANGLE, 62, "rotation_key_src.angle", 4, 8, 1),
    (ROTATION_KEY_SRC_ORIGIN_X, 63, "rotation_key_src.origin_x", 4, 8, 1),
    (ROTATION_KEY_SRC_ORIGIN_Y, 64, "rotation_key_src.origin_y", 4, 8, 1),
    (ROTATION_KEY_SRC_SCALE, 65, "rotation_key_src.scale", 4, 8, 1),
    (ROTATION_KEY_SRC_REFLECT_X, 66, "rotation_key_src.reflect_x", 4, 8, 1),
    (ROTATION_KEY_SRC_REFLECT_Y, 67, "rotation_key_src.reflect_y", 4, 8, 1),
    (ART_MESH_KEY_SRC_OPACITY, 68, "art_mesh_key_src.opacity", 4, 9, 1),
    (ART_MESH_KEY_SRC_DRAW_ORDER, 69, "art_mesh_key_src.draw_order", 4, 9, 1),
    (ART_MESH_KEY_SRC_KEY_POS_OFF, 70, "art_mesh_key_src.key_pos_off", 4, 9, 1),
    (KEY_POS_SRC_XY, 71, "key_pos_src.xy", 4, 10, 1),
    (KEY_TABLE_IDX_SRC_IDX, 72, "key_table_idx_src.idx", 4, 11, 1),
    (BINDING_SRC_KEY_TABLE_IDX_OFF, 73, "binding_src.key_table_idx_off", 4, 12, 1),
    (BINDING_SRC_KEY_TABLE_IDX_LEN, 74, "binding_src.key_table_idx_len", 4, 12, 1),
    (KEY_TABLE_SRC_KEYS_OFF, 75, "key_table_src.keys_off", 4, 13, 1),
    (KEY_TABLE_SRC_KEYS_LEN, 76, "key_table_src.keys_len", 4, 13, 1),
    (KEYS_SRC_KEY, 77, "keys_src.key", 4, 14, 1),
    (UV_SRC_XY, 78, "uv_src.xy", 4, 15, 1),
    (IDX_SRC_IDX, 79, "idx_src.idx", 2, 16, 1),
    (MASK_SRC_ART_MESH_IDX, 80, "mask_src.art_mesh_idx", 4, 17, 1),
    (DRAW_GROUP_SRC_OBJ_OFF, 81, "draw_group_src.obj_off", 4, 18, 1),
    (DRAW_GROUP_SRC_OBJ_LEN, 82, "draw_group_src.obj_len", 4, 18, 1),
    (DRAW_GROUP_SRC_OBJ_TOTAL_COUNT, 83, "draw_group_src.obj_total_count", 4, 18, 1),
    (DRAW_GROUP_SRC_MAX_ORDER, 84, "draw_group_src.max_order", 4, 18, 1),
    (DRAW_GROUP_SRC_MIN_ORDER, 85, "draw_group_src.min_order", 4, 18, 1),
    (DRAW_GROUP_OBJ_SRC_TYPE, 86, "draw_group_obj_src.type", 4, 19, 1),
    (DRAW_GROUP_OBJ_SRC_IDX, 87, "draw_group_obj_src.idx", 4, 19, 1),
    (DRAW_GROUP_OBJ_SRC_SELF_GROUP_IDX, 88, "draw_group_obj_src.self_group_idx", 4, 19, 1),
    (GLUE_SRC_ID_RUNTIME, 89, "glue_src.id_runtime", 8, 20, 1),
    (GLUE_SRC_ID, 90, "glue_src.id", 64, 20, 1),
    (GLUE_SRC_BINDING_IDX, 91, "glue_src.binding_idx", 4, 20, 1),
    (GLUE_SRC_KEYFORM_OFF, 92, "glue_src.keyform_off", 4, 20, 1),
    (GLUE_SRC_KEY_LEN, 93, "glue_src.key_len", 4, 20, 1),
    (GLUE_SRC_ART_MESH_IDX_A, 94, "glue_src.art_mesh_idx_a", 4, 20, 1),
    (GLUE_SRC_ART_MESH_IDX_B, 95, "glue_src.art_mesh_idx_b", 4, 20, 1),
    (GLUE_SRC_INFO_OFF, 96, "glue_src.info_off", 4, 20, 1),
    (GLUE_SRC_INFO_LEN, 97, "glue_src.info_len", 4, 20, 1),
    (GLUE_INFO_SRC_WEIGHT, 98, "glue_info_src.weight", 4, 21, 1),
    (GLUE_INFO_SRC_POS_IDX, 99, "glue_info_src.pos_idx", 2, 21, 1),
    (GLUE_KEY_SRC_INTENSITY, 100, "glue_key_src.intensity", 4, 22, 1),
    (WARP_SRC_QUAD_TRANSFORM, 101, "warp_src.quad_transform", 4, 2, 2),
    (PARAM_KEYS_SRC_KEY_RUNTIME, 102, "param_keys_src.key_runtime", 8, 5, 4),
    (PARAM_KEYS_SRC_KEYS_OFF, 103, "param_keys_src.keys_off", 4, 5, 4),
    (PARAM_KEYS_SRC_KEYS_LEN, 104, "param_keys_src.keys_len", 4, 5, 4),
    (WARP_SRC_KEY_COLOR_OFF, 105, "warp_src.key_color_off", 4, 2, 4),
    (ROTATION_SRC_KEY_COLOR_OFF, 106, "rotation_src.key_color_off", 4, 3, 4),
    (ART_MESH_SRC_KEY_COLOR_OFF, 107, "art_mesh_src.key_color_off", 4, 4, 4),
    (KEYFORM_MUL_COLOR_SRC_R, 108, "keyform_mul_color_src.r", 4, 23, 4),
    (KEYFORM_MUL_COLOR_SRC_G, 109, "keyform_mul_color_src.g", 4, 23, 4),
    (KEYFORM_MUL_COLOR_SRC_B, 110, "keyform_mul_color_src.b", 4, 23, 4),
    (KEYFORM_SCR_COLOR_SRC_R, 111, "keyform_scr_color_src.r", 4, 24, 4),
    (KEYFORM_SCR_COLOR_SRC_G, 112, "keyform_scr_color_src.g", 4, 24, 4),
    (KEYFORM_SCR_COLOR_SRC_B, 113, "keyform_scr_color_src.b", 4, 24, 4),
    (PARAM_SRC_TYPE, 114, "param_src.type", 4, 5, 4),
    (PARAM_SRC_BLEND_KEY_TABLE_OFF, 115, "param_src.blend_key_table_off", 4, 5, 4),
    (PARAM_SRC_BLEND_KEY_TABLE_LEN, 116, "param_src.blend_key_table_len", 4, 5, 4),
    (BLEND_KEY_TABLE_SRC_KEYS_OFF, 117, "blend_key_table_src.keys_off", 4, 25, 4),
    (BLEND_KEY_TABLE_SRC_KEYS_LEN, 118, "blend_key_table_src.keys_len", 4, 25, 4),
    (BLEND_KEY_TABLE_SRC_BASE_KEY_IDX, 119, "blend_key_table_src.base_key_idx", 4, 25, 4),
    (BLEND_BINDING_SRC_KEY_TABLE_IDX, 120, "blend_binding_src.key_table_idx", 4, 26, 4),
    (BLEND_BINDING_SRC_KEY_BS_OFF, 121, "blend_binding_src.key_bs_off", 4, 26, 4),
    (BLEND_BINDING_SRC_KEY_BS_LEN, 122, "blend_binding_src.key_bs_len", 4, 26, 4),
    (BLEND_BINDING_SRC_BS_CONSTRAINT_IDX_OFF, 123, "blend_binding_src.bs_constraint_idx_off", 4, 26, 4),
    (BLEND_BINDING_SRC_BS_CONSTRAINT_IDX_LEN, 124, "blend_binding_src.bs_constraint_idx_len", 4, 26, 4),
    (BS_WARP_SRC_TARGET_IDX, 125, "bs_warp_src.target_idx", 4, 27, 4),
    (BS_WARP_SRC_BS_BINDING_OFF, 126, "bs_warp_src.bs_binding_off", 4, 27, 4),
    (BS_WARP_SRC_BS_BINDING_LEN, 127, "bs_warp_src.bs_binding_len", 4, 27, 4),
    (BS_ART_MESH_SRC_TARGET_IDX, 128, "bs_art_mesh_src.target_idx", 4, 28, 4),
    (BS_ART_MESH_SRC_BS_BINDING_OFF, 129, "bs_art_mesh_src.bs_binding_off", 4, 28, 4),
    (BS_ART_MESH_SRC_BS_BINDING_LEN, 130, "bs_art_mesh_src.bs_binding_len", 4, 28, 4),
    (BLEND_CONSTRAINT_IDX_SRC_CONSTRAINT_IDX, 131, "blend_constraint_idx_src.constraint_idx", 4, 29, 4),
    (BLEND_CONSTRAINT_SRC_PARAMETER_IDX, 132, "blend_constraint_src.parameter_idx", 4, 30, 4),
    (BLEND_CONSTRAINT_SRC_VALUE_OFF, 133, "blend_constraint_src.value_off", 4, 30, 4),
    (BLEND_CONSTRAINT_SRC_VALUE_LEN, 134, "blend_constraint_src.value_len", 4, 30, 4),
    (BLEND_CONSTRAINT_VAL_SRC_KEY, 135, "blend_constraint_val_src.key", 4, 31, 4),
    (BLEND_CONSTRAINT_VAL_SRC_WEIGHT, 136, "blend_constraint_val_src.weight", 4, 31, 4),
    (WARP_KEY_SRC_KEY_MUL_COLOR_OFF, 137, "warp_key_src.key_mul_color_off", 4, 7, 5),
    (WARP_KEY_SRC_KEY_SCR_COLOR_OFF, 138, "warp_key_src.key_scr_color_off", 4, 7, 5),
    (ROTATION_KEY_SRC_KEY_MUL_COLOR_OFF, 139, "rotation_key_src.key_mul_color_off", 4, 8, 5),
    (ROTATION_KEY_SRC_KEY_SCR_COLOR_OFF, 140, "rotation_key_src.key_scr_color_off", 4, 8, 5),
    (ART_MESH_KEY_SRC_KEY_MUL_COLOR_OFF, 141, "art_mesh_key_src.key_mul_color_off", 4, 9, 5),
    (ART_MESH_KEY_SRC_KEY_SCR_COLOR_OFF, 142, "art_mesh_key_src.key_scr_color_off", 4, 9, 5),
    (BS_PART_SRC_TARGET_IDX, 143, "bs_part_src.target_idx", 4, 32, 5),
    (BS_PART_SRC_BS_BINDING_OFF, 144, "bs_part_src.bs_binding_off", 4, 32, 5),
    (BS_PART_SRC_BS_BINDING_LEN, 145, "bs_part_src.bs_binding_len", 4, 32, 5),
    (BS_ROTATION_SRC_TARGET_IDX, 146, "bs_rotation_src.target_idx", 4, 33, 5),
    (BS_ROTATION_SRC_BS_BINDING_OFF, 147, "bs_rotation_src.bs_binding_off", 4, 33, 5),
    (BS_ROTATION_SRC_BS_BINDING_LEN, 148, "bs_rotation_src.bs_binding_len", 4, 33, 5),
    (BS_GLUE_SRC_TARGET_IDX, 149, "bs_glue_src.target_idx", 4, 34, 5),
    (BS_GLUE_SRC_BS_BINDING_OFF, 150, "bs_glue_src.bs_binding_off", 4, 34, 5),
    (BS_GLUE_SRC_BS_BINDING_LEN, 151, "bs_glue_src.bs_binding_len", 4, 34, 5),
    (PART_SRC_OFFSCREEN_IDX, 152, "part_src.offscreen_idx", 4, 0, 6),
    (ART_MESH_SRC_BLEND_MODE, 153, "art_mesh_src.blend_mode", 4, 4, 6),
    (OFFSCREEN_SRC_DRAWABLE_MASK_RUNTIME, 154, "offscreen_src.drawable_mask_runtime", 8, 35, 6),
    (OFFSCREEN_SRC_OWNER_IDX, 155, "offscreen_src.owner_idx", 4, 35, 6),
    (OFFSCREEN_SRC_DRAWABLE_FLAG, 156, "offscreen_src.drawable_flag", 1, 35, 6),
    (OFFSCREEN_SRC_BLEND_MODE, 157, "offscreen_src.blend_mode", 4, 35, 6),
    (OFFSCREEN_SRC_MASK_OFF, 158, "offscreen_src.mask_off", 4, 35, 6),
    (OFFSCREEN_SRC_MASK_LEN, 159, "offscreen_src.mask_len", 4, 35, 6),
    (PART_KEY_SRC_KEY_IDX, 160, "part_key_src.key_idx", 4, 6, 6),
    (OFFSCREEN_KEY_SRC_OPACITY, 161, "offscreen_key_src.opacity", 4, 36, 6),
    (OFFSCREEN_KEY_SRC_KEY_MUL_COLOR_OFF, 162, "offscreen_key_src.key_mul_color_off", 4, 36, 6),
    (OFFSCREEN_KEY_SRC_KEY_SCR_COLOR_OFF, 163, "offscreen_key_src.key_scr_color_off", 4, 36, 6),
    (BS_OFFSCREEN_SRC_TARGET_IDX, 164, "bs_offscreen_src.target_idx", 4, 37, 6),
    (BS_OFFSCREEN_SRC_BS_BINDING_OFF, 165, "bs_offscreen_src.bs_binding_off", 4, 37, 6),
    (BS_OFFSCREEN_SRC_BS_BINDING_LEN, 166, "bs_offscreen_src.bs_binding_len", 4, 37, 6),
}

#[derive(Debug, Clone, Copy)]
pub struct VersionLayout {
    pub version: u8,
}
impl VersionLayout {
    pub fn new(version: u8) -> Result<Self, Status> {
        if !(1..=6).contains(&version) {
            return Err(Status::error(
                "UNSUPPORTED_VERSION",
                format!("MOC3 version {version}"),
            ));
        }
        Ok(Self { version })
    }
    pub fn offset_count(self) -> usize {
        if self.version >= 6 {
            480
        } else {
            160
        }
    }
    pub fn count_count(self) -> usize {
        if self.version >= 5 {
            64
        } else {
            32
        }
    }
    pub fn section_count(self) -> usize {
        SCHEMA
            .iter()
            .take_while(|s| s.minimum_version <= self.version)
            .count()
    }
    pub fn loader_reserve(self) -> usize {
        64 + self.offset_count() * 12
    }
    pub fn require(self, field: usize) -> Result<&'static SectionSchema, Status> {
        SCHEMA
            .get(field)
            .filter(|s| s.minimum_version <= self.version)
            .ok_or_else(|| {
                Status::error(
                    "UNSUPPORTED_FIELD",
                    format!("section {field} is unavailable in version {}", self.version),
                )
            })
    }
}
