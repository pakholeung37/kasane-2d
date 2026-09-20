// MOC3 v5 schema, derived from PurismCore src/moc3.h (MIT).
// Copyright (c) 2026 Sakura Motion Project

#[derive(Debug, Clone, Copy)]
pub struct SectionSchema {
    pub name: &'static str,
    pub width: usize,
    pub count_index: i32,
}

pub const SCHEMA: &[SectionSchema] = &[
    SectionSchema {
        name: "count_info",
        width: 256,
        count_index: -1,
    },
    SectionSchema {
        name: "canvas_info",
        width: 24,
        count_index: -1,
    },
    SectionSchema {
        name: "part_src.id_runtime",
        width: 8,
        count_index: 0,
    },
    SectionSchema {
        name: "part_src.id",
        width: 64,
        count_index: 0,
    },
    SectionSchema {
        name: "part_src.binding_idx",
        width: 4,
        count_index: 0,
    },
    SectionSchema {
        name: "part_src.keyform_off",
        width: 4,
        count_index: 0,
    },
    SectionSchema {
        name: "part_src.key_len",
        width: 4,
        count_index: 0,
    },
    SectionSchema {
        name: "part_src.visible",
        width: 4,
        count_index: 0,
    },
    SectionSchema {
        name: "part_src.enable",
        width: 4,
        count_index: 0,
    },
    SectionSchema {
        name: "part_src.parent_part_idx",
        width: 4,
        count_index: 0,
    },
    SectionSchema {
        name: "deformer_src.id_runtime",
        width: 8,
        count_index: 1,
    },
    SectionSchema {
        name: "deformer_src.id",
        width: 64,
        count_index: 1,
    },
    SectionSchema {
        name: "deformer_src.binding_idx",
        width: 4,
        count_index: 1,
    },
    SectionSchema {
        name: "deformer_src.visible",
        width: 4,
        count_index: 1,
    },
    SectionSchema {
        name: "deformer_src.enable",
        width: 4,
        count_index: 1,
    },
    SectionSchema {
        name: "deformer_src.parent_part_idx",
        width: 4,
        count_index: 1,
    },
    SectionSchema {
        name: "deformer_src.parent_deformer_idx",
        width: 4,
        count_index: 1,
    },
    SectionSchema {
        name: "deformer_src.type",
        width: 4,
        count_index: 1,
    },
    SectionSchema {
        name: "deformer_src.local_idx",
        width: 4,
        count_index: 1,
    },
    SectionSchema {
        name: "warp_src.binding_idx",
        width: 4,
        count_index: 2,
    },
    SectionSchema {
        name: "warp_src.keyform_off",
        width: 4,
        count_index: 2,
    },
    SectionSchema {
        name: "warp_src.key_len",
        width: 4,
        count_index: 2,
    },
    SectionSchema {
        name: "warp_src.vertex_count",
        width: 4,
        count_index: 2,
    },
    SectionSchema {
        name: "warp_src.row",
        width: 4,
        count_index: 2,
    },
    SectionSchema {
        name: "warp_src.col",
        width: 4,
        count_index: 2,
    },
    SectionSchema {
        name: "rotation_src.binding_idx",
        width: 4,
        count_index: 3,
    },
    SectionSchema {
        name: "rotation_src.keyform_off",
        width: 4,
        count_index: 3,
    },
    SectionSchema {
        name: "rotation_src.key_len",
        width: 4,
        count_index: 3,
    },
    SectionSchema {
        name: "rotation_src.base_angle",
        width: 4,
        count_index: 3,
    },
    SectionSchema {
        name: "art_mesh_src.id_runtime",
        width: 8,
        count_index: 4,
    },
    SectionSchema {
        name: "art_mesh_src.uv_runtime",
        width: 8,
        count_index: 4,
    },
    SectionSchema {
        name: "art_mesh_src.pos_idx_runtime",
        width: 8,
        count_index: 4,
    },
    SectionSchema {
        name: "art_mesh_src.drawable_mask_runtime",
        width: 8,
        count_index: 4,
    },
    SectionSchema {
        name: "art_mesh_src.id",
        width: 64,
        count_index: 4,
    },
    SectionSchema {
        name: "art_mesh_src.binding_idx",
        width: 4,
        count_index: 4,
    },
    SectionSchema {
        name: "art_mesh_src.keyform_off",
        width: 4,
        count_index: 4,
    },
    SectionSchema {
        name: "art_mesh_src.key_len",
        width: 4,
        count_index: 4,
    },
    SectionSchema {
        name: "art_mesh_src.visible",
        width: 4,
        count_index: 4,
    },
    SectionSchema {
        name: "art_mesh_src.enable",
        width: 4,
        count_index: 4,
    },
    SectionSchema {
        name: "art_mesh_src.parent_part_idx",
        width: 4,
        count_index: 4,
    },
    SectionSchema {
        name: "art_mesh_src.parent_deformer_idx",
        width: 4,
        count_index: 4,
    },
    SectionSchema {
        name: "art_mesh_src.texture_no",
        width: 4,
        count_index: 4,
    },
    SectionSchema {
        name: "art_mesh_src.drawable_flag",
        width: 1,
        count_index: 4,
    },
    SectionSchema {
        name: "art_mesh_src.vertex_count",
        width: 4,
        count_index: 4,
    },
    SectionSchema {
        name: "art_mesh_src.uv_off",
        width: 4,
        count_index: 4,
    },
    SectionSchema {
        name: "art_mesh_src.idx_off",
        width: 4,
        count_index: 4,
    },
    SectionSchema {
        name: "art_mesh_src.idx_len",
        width: 4,
        count_index: 4,
    },
    SectionSchema {
        name: "art_mesh_src.mask_off",
        width: 4,
        count_index: 4,
    },
    SectionSchema {
        name: "art_mesh_src.mask_len",
        width: 4,
        count_index: 4,
    },
    SectionSchema {
        name: "param_src.id_runtime",
        width: 8,
        count_index: 5,
    },
    SectionSchema {
        name: "param_src.id",
        width: 64,
        count_index: 5,
    },
    SectionSchema {
        name: "param_src.maximum_value",
        width: 4,
        count_index: 5,
    },
    SectionSchema {
        name: "param_src.minimum_value",
        width: 4,
        count_index: 5,
    },
    SectionSchema {
        name: "param_src.default_value",
        width: 4,
        count_index: 5,
    },
    SectionSchema {
        name: "param_src.repeat",
        width: 4,
        count_index: 5,
    },
    SectionSchema {
        name: "param_src.decimal_places",
        width: 4,
        count_index: 5,
    },
    SectionSchema {
        name: "param_src.key_table_off",
        width: 4,
        count_index: 5,
    },
    SectionSchema {
        name: "param_src.key_table_len",
        width: 4,
        count_index: 5,
    },
    SectionSchema {
        name: "part_key_src.draw_order",
        width: 4,
        count_index: 6,
    },
    SectionSchema {
        name: "warp_key_src.opacity",
        width: 4,
        count_index: 7,
    },
    SectionSchema {
        name: "warp_key_src.key_pos_off",
        width: 4,
        count_index: 7,
    },
    SectionSchema {
        name: "rotation_key_src.opacity",
        width: 4,
        count_index: 8,
    },
    SectionSchema {
        name: "rotation_key_src.angle",
        width: 4,
        count_index: 8,
    },
    SectionSchema {
        name: "rotation_key_src.origin_x",
        width: 4,
        count_index: 8,
    },
    SectionSchema {
        name: "rotation_key_src.origin_y",
        width: 4,
        count_index: 8,
    },
    SectionSchema {
        name: "rotation_key_src.scale",
        width: 4,
        count_index: 8,
    },
    SectionSchema {
        name: "rotation_key_src.reflect_x",
        width: 4,
        count_index: 8,
    },
    SectionSchema {
        name: "rotation_key_src.reflect_y",
        width: 4,
        count_index: 8,
    },
    SectionSchema {
        name: "art_mesh_key_src.opacity",
        width: 4,
        count_index: 9,
    },
    SectionSchema {
        name: "art_mesh_key_src.draw_order",
        width: 4,
        count_index: 9,
    },
    SectionSchema {
        name: "art_mesh_key_src.key_pos_off",
        width: 4,
        count_index: 9,
    },
    SectionSchema {
        name: "key_pos_src.xy",
        width: 4,
        count_index: 10,
    },
    SectionSchema {
        name: "key_table_idx_src.idx",
        width: 4,
        count_index: 11,
    },
    SectionSchema {
        name: "binding_src.key_table_idx_off",
        width: 4,
        count_index: 12,
    },
    SectionSchema {
        name: "binding_src.key_table_idx_len",
        width: 4,
        count_index: 12,
    },
    SectionSchema {
        name: "key_table_src.keys_off",
        width: 4,
        count_index: 13,
    },
    SectionSchema {
        name: "key_table_src.keys_len",
        width: 4,
        count_index: 13,
    },
    SectionSchema {
        name: "keys_src.key",
        width: 4,
        count_index: 14,
    },
    SectionSchema {
        name: "uv_src.xy",
        width: 4,
        count_index: 15,
    },
    SectionSchema {
        name: "idx_src.idx",
        width: 2,
        count_index: 16,
    },
    SectionSchema {
        name: "mask_src.art_mesh_idx",
        width: 4,
        count_index: 17,
    },
    SectionSchema {
        name: "draw_group_src.obj_off",
        width: 4,
        count_index: 18,
    },
    SectionSchema {
        name: "draw_group_src.obj_len",
        width: 4,
        count_index: 18,
    },
    SectionSchema {
        name: "draw_group_src.obj_total_count",
        width: 4,
        count_index: 18,
    },
    SectionSchema {
        name: "draw_group_src.max_order",
        width: 4,
        count_index: 18,
    },
    SectionSchema {
        name: "draw_group_src.min_order",
        width: 4,
        count_index: 18,
    },
    SectionSchema {
        name: "draw_group_obj_src.type",
        width: 4,
        count_index: 19,
    },
    SectionSchema {
        name: "draw_group_obj_src.idx",
        width: 4,
        count_index: 19,
    },
    SectionSchema {
        name: "draw_group_obj_src.self_group_idx",
        width: 4,
        count_index: 19,
    },
    SectionSchema {
        name: "glue_src.id_runtime",
        width: 8,
        count_index: 20,
    },
    SectionSchema {
        name: "glue_src.id",
        width: 64,
        count_index: 20,
    },
    SectionSchema {
        name: "glue_src.binding_idx",
        width: 4,
        count_index: 20,
    },
    SectionSchema {
        name: "glue_src.keyform_off",
        width: 4,
        count_index: 20,
    },
    SectionSchema {
        name: "glue_src.key_len",
        width: 4,
        count_index: 20,
    },
    SectionSchema {
        name: "glue_src.art_mesh_idx_a",
        width: 4,
        count_index: 20,
    },
    SectionSchema {
        name: "glue_src.art_mesh_idx_b",
        width: 4,
        count_index: 20,
    },
    SectionSchema {
        name: "glue_src.info_off",
        width: 4,
        count_index: 20,
    },
    SectionSchema {
        name: "glue_src.info_len",
        width: 4,
        count_index: 20,
    },
    SectionSchema {
        name: "glue_info_src.weight",
        width: 4,
        count_index: 21,
    },
    SectionSchema {
        name: "glue_info_src.pos_idx",
        width: 2,
        count_index: 21,
    },
    SectionSchema {
        name: "glue_key_src.intensity",
        width: 4,
        count_index: 22,
    },
    SectionSchema {
        name: "warp_src.quad_transform",
        width: 4,
        count_index: 2,
    },
    SectionSchema {
        name: "param_keys_src.key_runtime",
        width: 8,
        count_index: 5,
    },
    SectionSchema {
        name: "param_keys_src.keys_off",
        width: 4,
        count_index: 5,
    },
    SectionSchema {
        name: "param_keys_src.keys_len",
        width: 4,
        count_index: 5,
    },
    SectionSchema {
        name: "warp_src.key_color_off",
        width: 4,
        count_index: 2,
    },
    SectionSchema {
        name: "rotation_src.key_color_off",
        width: 4,
        count_index: 3,
    },
    SectionSchema {
        name: "art_mesh_src.key_color_off",
        width: 4,
        count_index: 4,
    },
    SectionSchema {
        name: "keyform_mul_color_src.r",
        width: 4,
        count_index: 23,
    },
    SectionSchema {
        name: "keyform_mul_color_src.g",
        width: 4,
        count_index: 23,
    },
    SectionSchema {
        name: "keyform_mul_color_src.b",
        width: 4,
        count_index: 23,
    },
    SectionSchema {
        name: "keyform_scr_color_src.r",
        width: 4,
        count_index: 24,
    },
    SectionSchema {
        name: "keyform_scr_color_src.g",
        width: 4,
        count_index: 24,
    },
    SectionSchema {
        name: "keyform_scr_color_src.b",
        width: 4,
        count_index: 24,
    },
    SectionSchema {
        name: "param_src.type",
        width: 4,
        count_index: 5,
    },
    SectionSchema {
        name: "param_src.blend_key_table_off",
        width: 4,
        count_index: 5,
    },
    SectionSchema {
        name: "param_src.blend_key_table_len",
        width: 4,
        count_index: 5,
    },
    SectionSchema {
        name: "blend_key_table_src.keys_off",
        width: 4,
        count_index: 25,
    },
    SectionSchema {
        name: "blend_key_table_src.keys_len",
        width: 4,
        count_index: 25,
    },
    SectionSchema {
        name: "blend_key_table_src.base_key_idx",
        width: 4,
        count_index: 25,
    },
    SectionSchema {
        name: "blend_binding_src.key_table_idx",
        width: 4,
        count_index: 26,
    },
    SectionSchema {
        name: "blend_binding_src.key_bs_off",
        width: 4,
        count_index: 26,
    },
    SectionSchema {
        name: "blend_binding_src.key_bs_len",
        width: 4,
        count_index: 26,
    },
    SectionSchema {
        name: "blend_binding_src.bs_constraint_idx_off",
        width: 4,
        count_index: 26,
    },
    SectionSchema {
        name: "blend_binding_src.bs_constraint_idx_len",
        width: 4,
        count_index: 26,
    },
    SectionSchema {
        name: "bs_warp_src.target_idx",
        width: 4,
        count_index: 27,
    },
    SectionSchema {
        name: "bs_warp_src.bs_binding_off",
        width: 4,
        count_index: 27,
    },
    SectionSchema {
        name: "bs_warp_src.bs_binding_len",
        width: 4,
        count_index: 27,
    },
    SectionSchema {
        name: "bs_art_mesh_src.target_idx",
        width: 4,
        count_index: 28,
    },
    SectionSchema {
        name: "bs_art_mesh_src.bs_binding_off",
        width: 4,
        count_index: 28,
    },
    SectionSchema {
        name: "bs_art_mesh_src.bs_binding_len",
        width: 4,
        count_index: 28,
    },
    SectionSchema {
        name: "blend_constraint_idx_src.constraint_idx",
        width: 4,
        count_index: 29,
    },
    SectionSchema {
        name: "blend_constraint_src.parameter_idx",
        width: 4,
        count_index: 30,
    },
    SectionSchema {
        name: "blend_constraint_src.value_off",
        width: 4,
        count_index: 30,
    },
    SectionSchema {
        name: "blend_constraint_src.value_len",
        width: 4,
        count_index: 30,
    },
    SectionSchema {
        name: "blend_constraint_val_src.key",
        width: 4,
        count_index: 31,
    },
    SectionSchema {
        name: "blend_constraint_val_src.weight",
        width: 4,
        count_index: 31,
    },
    SectionSchema {
        name: "warp_key_src.key_mul_color_off",
        width: 4,
        count_index: 7,
    },
    SectionSchema {
        name: "warp_key_src.key_scr_color_off",
        width: 4,
        count_index: 7,
    },
    SectionSchema {
        name: "rotation_key_src.key_mul_color_off",
        width: 4,
        count_index: 8,
    },
    SectionSchema {
        name: "rotation_key_src.key_scr_color_off",
        width: 4,
        count_index: 8,
    },
    SectionSchema {
        name: "art_mesh_key_src.key_mul_color_off",
        width: 4,
        count_index: 9,
    },
    SectionSchema {
        name: "art_mesh_key_src.key_scr_color_off",
        width: 4,
        count_index: 9,
    },
    SectionSchema {
        name: "bs_part_src.target_idx",
        width: 4,
        count_index: 32,
    },
    SectionSchema {
        name: "bs_part_src.bs_binding_off",
        width: 4,
        count_index: 32,
    },
    SectionSchema {
        name: "bs_part_src.bs_binding_len",
        width: 4,
        count_index: 32,
    },
    SectionSchema {
        name: "bs_rotation_src.target_idx",
        width: 4,
        count_index: 33,
    },
    SectionSchema {
        name: "bs_rotation_src.bs_binding_off",
        width: 4,
        count_index: 33,
    },
    SectionSchema {
        name: "bs_rotation_src.bs_binding_len",
        width: 4,
        count_index: 33,
    },
    SectionSchema {
        name: "bs_glue_src.target_idx",
        width: 4,
        count_index: 34,
    },
    SectionSchema {
        name: "bs_glue_src.bs_binding_off",
        width: 4,
        count_index: 34,
    },
    SectionSchema {
        name: "bs_glue_src.bs_binding_len",
        width: 4,
        count_index: 34,
    },
];
