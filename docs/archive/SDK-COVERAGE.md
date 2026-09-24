# Document Bridge → SDK 覆盖清单

本表记录 Document Bridge 到 SDK 的能力映射；当前可运行入口以 [SDK-API.md](SDK-API.md) 和 `modules/kasane-sdk/tests/` 为准。`session_mut`、`apply`、`evaluated_frame` 与内部发布机制是旧宿主实现细节，不作为同名 SDK API 迁移；generation 和 object epoch 由新 `Version`/`ObjectHandle` 表达。

| 旧公开入口 | 新 SDK 入口或方向 | 状态 / 测试 |
| --- | --- | --- |
| `initialize` | `AuthoringSession::new` | 已实现 / `vertical_slice` |
| `new_project` | `AuthoringSession::new_project` 原子替换文档 | 已实现 / `document_operations` |
| `add_image_asset` | `EditSession::create_asset` | 已实现 / `vertical_slice` |
| `get_asset_snapshot` | `AuthoringSession::asset` | 已实现 / `vertical_slice` |
| `begin_transaction` | `begin_edit` | 已实现 / `vertical_slice` |
| `commit_transaction` | `EditSession::commit` | 已实现 / `vertical_slice` |
| `cancel_transaction` | drop edit | 已实现 / `vertical_slice` |
| `get_history_state` | `history_state`（步数和容量） | 已实现 / `history_budget` |
| `begin_action` | `begin_edit(label)` | 已实现 / `vertical_slice` |
| `end_action` | `commit` | 已实现 / `vertical_slice` |
| `cancel_action` | drop edit | 已实现 / `vertical_slice` |
| `undo` | `AuthoringSession::undo` | 已实现 / `vertical_slice` |
| `redo` | `AuthoringSession::redo` | 已实现 / `vertical_slice` |
| `create_mesh` | `EditSession::create_mesh` | 已实现 / `vertical_slice` |
| `replace_mesh` | `EditSession::replace_mesh` | 已实现 / `mesh_editing` |
| `replace_mesh_with_keyforms` | mesh + 完整 binding 单批次 | 部分：`replace_topology` 显式依赖 / `mesh_editing` |
| `get_mesh` | `AuthoringSession::mesh` | 已实现 / `vertical_slice` |
| `replace_mesh_topology` | `EditSession::replace_topology` | 已实现 / `mesh_editing` |
| `get_mesh_topology_snapshot` | 带 version 的 `geometry` | 已实现 / `mesh_editing` |
| `set_mesh_keyform` | `EditSession::set_mesh_keyform` | 已实现 / `binding_authoring` |
| `set_mesh_properties` | `EditSession::update_mesh_properties` | 已实现 / `mesh_editing` |
| `set_vertex_positions` | `EditSession::update_positions` | 已实现 / `vertical_slice` |
| `rename_mesh` | `EditSession::rename_mesh` | 已实现 / `mesh_editing` |
| `stage_vertex_positions` | `update_positions` + edit | 已实现 / `vertical_slice` |
| `commit_vertex_updates` | `commit` + `expected_version` | 已实现 / `vertical_slice` |
| `get_mesh_snapshot` | `mesh` / `geometry` | 已实现 / `vertical_slice` |
| `create_parameter` | `EditSession::create_parameter` | 已实现 / `binding_authoring` |
| `replace_parameter` | `EditSession::replace_parameter` | 已实现 / `binding_authoring` |
| `get_parameter` | `AuthoringSession::parameter` | 已实现 / `binding_authoring` |
| `set_preview_values` | `AuthoringSession::set_preview_values` | 已实现 / `scene_authoring` |
| `set_preview_parameter` | `AuthoringSession::set_preview_parameter` | 已实现 / `scene_authoring` |
| `reset_preview_values` | `AuthoringSession::reset_preview_values` | 已实现 / `scene_authoring` |
| `get_parameter_samples` | `evaluate` 的 requested/actual | 部分 / `binding_authoring` |
| `create_rotation` | `create_transform(TransformData::Rotation)` | 已实现，暂用完整描述 / `scene_authoring` |
| `create_warp` | `create_transform(TransformData::Warp)` | 已实现，暂用完整描述 / `scene_authoring` |
| `set_rotation` | `EditSession::update_rotation` | 已实现 / `scene_authoring` |
| `set_warp_points` | `EditSession::update_warp_points` | 已实现 / `scene_authoring` |
| `set_deform_parent` | `EditSession::set_deform_parent` | 已实现 / `scene_authoring` |
| `set_organization_parent` | `set_organization_parent`、`set_transform_part`、`set_mesh_part` | 已实现 / `scene_authoring` |
| `get_deformer_snapshot` | `AuthoringSession::transform` | 部分：返回副本，无单独版本字段 |
| `get_deformer` | `AuthoringSession::transform` | 已实现 / `scene_authoring` |
| `write_transform` | `create_transform/replace_transform` | 已实现 / `scene_authoring` |
| `write_part` | `create_part/replace_part` | 已实现 / `scene_authoring` |
| `replace_part_binding_with_offscreen` | `EditSession::replace_part_binding_with_offscreen` | 已实现 / `effects_authoring` |
| `write_scene_binding` | `create_scene_binding/replace_scene_binding/set_scene_keyform` | 已实现 / `scene_authoring` |
| `write_binding` | `EditSession::create_binding/replace_binding` | 已实现 / `binding_authoring` |
| `write_blend_key_table` | `create_blend_key_table/replace_blend_key_table` | 已实现 / `effects_authoring` |
| `get_blend_key_table_snapshot` | `AuthoringSession::blend_key_table` | 已实现 / `effects_authoring` |
| `write_blend_constraint` | `create_blend_constraint/replace_blend_constraint` | 已实现 / `effects_authoring` |
| `get_blend_constraint_snapshot` | `AuthoringSession::blend_constraint` | 已实现 / `effects_authoring` |
| `write_blend_binding` | `create_blend_binding/replace_blend_binding` | 已实现 / `effects_authoring` |
| `get_blend_binding_snapshot` | `AuthoringSession::blend_binding` | 已实现 / `effects_authoring` |
| `write_glue` | `create_glue/replace_glue` | 已实现 / `effects_authoring` |
| `get_glue_snapshot` | `AuthoringSession::glue` | 已实现 / `effects_authoring` |
| `write_offscreen` | `create_offscreen/replace_offscreen` | 已实现 / `effects_authoring` |
| `get_offscreen_snapshot` | `AuthoringSession::offscreen` | 已实现 / `effects_authoring` |
| `references_to` | `AuthoringSession::references_to` | 已实现 / `document_operations` |
| `erase_object` | `EditSession::erase_object`（拒绝仍有引用） | 已实现 / `document_operations` |
| `evaluate_mesh` | `AuthoringSession::evaluate` + 查找 drawable | 部分 / `vertical_slice` |
| `get_frame` | `AuthoringSession::preview_frame` | 已实现 / `scene_authoring` |
| `get_document_state` | version/modified/history/preview | 部分 / `vertical_slice` |
| `get_document_summary` | `document_id/canvas/asset_ids/mesh_ids` 等 | 部分 / `vertical_slice` |

| 旧 ProjectIO 入口 | 新 SDK 方向 | 状态 |
| --- | --- | --- |
| `save_project` | `AuthoringSession::save_project`，含 save-as 和历史资源重定位 | 已实现 / `project_io` |
| `open_project` | `AuthoringSession::open_project` | 已实现 / `project_io` |
| `diagnose_resources` | `AuthoringSession::diagnose_resources` | 已实现 / `project_io` |
| `relocate_asset` | `prepare_relocated_asset` + `EditSession::replace_asset` | 已实现 / `project_io` |
| `replace_asset` | `prepare_png_asset` + `EditSession::replace_asset` | 已实现 / `project_io` |
| `export_package` | `AuthoringSession::export_package` | 已实现 / `project_io` |
| `import_model3` | `AuthoringSession::import_model3` + 导入报告 | 已实现 / `project_io` |
| `import_moc3` | `AuthoringSession::import_bare_moc3` + 显式纹理槽位映射 | 已实现 / `project_io` |
| `inspect_model` | import inspection | 部分：导入报告，无独立预检查入口 |

core 额外提供的 canvas 和 draw-order groups 读取/替换已由 `canvas`、`draw_order_groups`、`replace_canvas` 和 `replace_draw_order_groups` 接入，见 `document_operations`。观察入口在 S4 范围。

SDK 新增 `validate_structure` 和提交前全结构检查，core 侧覆盖 checkpoint 恢复后的校验；`diagnose_geometry` 提供与结构错误分开的创作提示。见 `document_operations` 和 core `structure` 单元测试。资源诊断由 `project_io` 覆盖。
