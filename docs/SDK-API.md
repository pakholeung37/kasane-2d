# Kasane SDK API（实施中）

状态：S0 契约初稿，S1 的内存对象族和预览入口已大体接入。尚未达到 [完整实施计划](SDK-IMPLEMENTATION-PLAN.md) 的 S1–S5 验收。

## 当前可运行的 Rust API

`kasane-sdk` 不依赖 Godot、Python、wgpu 或窗口。调用者传入规范小写、非零的文档和对象 UUID。当前不会自动生成 UUID，也没有持久化 alias。

```rust
use kasane_core::{Canvas, PreviewValues, Vec2};
use kasane_sdk::{AuthoringSession, prepare_png_asset, rectangle_mesh};

let mut session = AuthoringSession::new(
    "00000000-0000-4000-8000-000000000001",
    Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0),
)?;
let asset = prepare_png_asset(
    "00000000-0000-4000-8000-000000000002", "texture",
    std::path::Path::new("texture.png"),
)?;
let mesh = rectangle_mesh(
    "00000000-0000-4000-8000-000000000003", "face", &asset.id,
    Vec2::new(40.0, 40.0), Vec2::new(60.0, 60.0),
)?;
session.edit("create", None, |edit| {
    edit.create_asset(asset)?;
    edit.create_mesh(mesh)?;
    Ok(())
})?;
let geometry = session.geometry("00000000-0000-4000-8000-000000000003").unwrap();
let frame = session.evaluate(&PreviewValues::new())?;
session.undo()?;
session.redo()?;
# Ok::<(), kasane_sdk::SdkError>(())
```

`AuthoringSession::begin_edit(label, expected_version)` 创建隔离候选文档。`EditSession::commit` 发布一次 revision；丢弃 `EditSession` 回滚。任一方法失败后，即便调用者捕获错误，`commit` 仍返回 `EDIT_ABORTED`。`session.edit` 是自动提交的闭包形式。无内容变化的批次不产生历史或事件，也不清除 redo。history 默认最多 50 条、256 MiB；`with_history_limits` 可配置，`history_state` 与 `estimated_content_bytes` 提供容量估算。超预算在发布前失败，成功提交才淘汰旧条目；估算包括持久内容的集合、字符串和数组 capacity，并不是 RSS 硬上限。

读取返回对象副本。`geometry()` 的 `positions` 是源坐标：根 mesh 为 `CanvasPixels`，有变形父对象时为 `ParentLocal(parent_id)`；`vertex_ids` 是稳定顶点身份，`triangles` 引用这些 ID。`evaluate(values)` 不修改会话状态，输出 positions 是 Runtime 坐标，根对象转换公式为 `(x-origin.x)/ppu`、`(origin.y-y)/ppu`。UV 保留 core 约定；源数组不会被 renderer 的纹理翻转改写。

`Version` 为 `(session_id, generation, revision)`。目前新建会话 generation 为 1，尚未提供替换文档入口。批次 `expected_version` 检查三个字段；过期错误提供 expected 和 actual。`SdkError` 有 `code/message/operation/object_ids` 及可选字段路径、版本和 referrers；core 未提供字段路径时留空。`EditReceipt` 提供前后版本、直接对象 ID、变化种类与标签。`drain_events()` 目前返回成功内容提交及 undo/redo 的 receipt。

`ObjectHandle` 当前覆盖 asset、mesh、parameter、mesh binding、Part、Transform、SceneBinding、BlendShape key table/constraint/binding、Glue 和 Offscreen。`handle(kind, id)` 只获取已提交对象；`resolve_handle` 校验 session、generation、对象种类和 incarnation。普通字段修改保留句柄，undo 使对象消失后即使 redo 恢复，旧句柄也保持过期。`mesh_by_handle` 是当前的强类型读取入口，其余类型仍通过 ID 查询。拓扑快照带 `Version`；`replace_topology` 要求它来自 edit 开始时的同一版本和 mesh，并把顶点映射与所有相关 binding/glue 一次交给 core 校验。

`create` 与 `replace` 分开。当前可显式替换 asset/mesh/parameter/mesh binding；`update_mesh_properties` 保留 mesh ID、runtime ID 与几何；`rename_mesh` 的纯名称提交只推进文档 revision，保留 evaluation revision。`find_meshes_by_name` 返回所有匹配，`require_unique_mesh` 区分缺失和重名。参数绑定以完整 `MeshBinding`（全部轴和笛卡尔积 keyform）提交，不能先发布空表；`evaluate` 返回 requested、actual 和 clamp/repeat 后的采样值。

Part、Transform、SceneBinding 也有 `create`/`replace`、ID 列表与对象副本查询。Transform 用 core 的 `TransformData::Rotation` / `Warp` 强类型描述；`update_rotation` 和 `update_warp_points` 更新已有对应类型。SceneBinding 的 Warp、Rotation、Part track 须一次提交完整笛卡尔 keyform 表，`set_scene_keyform` 更新已有组合。`set_deform_parent` 修改 mesh 的变形父节点，`set_transform_parent` 修改 Transform 的变形父节点；`set_organization_parent` 修改 Part 的组织父节点，`set_transform_part` 和 `set_mesh_part` 修改所属 Part。这些操作保留现有局部坐标数值，由 core 校验目标和环。

会话预览由 `PreviewState` 缓存。`set_preview_values`、`set_preview_parameter` 和 `reset_preview_values` 在求值成功后才发布预览值；失败保留旧值、revision 和缓存帧。`preview_frame` 返回可共享的不可变 `Arc<DrawableFrame>`；相同预览值和 evaluation revision 会复用该帧。`evaluate(values)` 是独立求值，不改预览状态。普通 metadata 修改可复用较早的求值帧，此时帧的 `source_revision` 仍是其求值时的文档 revision。

`replace_canvas` 与 `replace_draw_order_groups` 也走候选批次。`draw_order_groups` 返回显式组的副本，文档没有显式组时返回 `None`。`references_to` 查询当前已发布文档的引用者；`erase_object` 在仍有引用时返回 `OBJECT_REFERENCED` 和 `referrers`。跨批次删除后即使用同一 ID 重建，旧句柄仍过期。

BlendShape key table、constraint、binding、Glue 和 Offscreen 均提供 `create`/`replace`、ID 列表与对象副本查询，强类型对象由 core 校验。Part binding 与 Offscreen 的 keyform 映射若需同时扩容，使用 `replace_part_binding_with_offscreen` 原子更新两者。

`prepare_png_asset` 读取 PNG、计算尺寸与 SHA-256，返回绝对路径资源描述；它不修改文档。`rectangle_mesh` 创建四顶点、两三角形的根 mesh 描述，UV 为四角。材质和坐标源字段仍可用 core 的强类型 `Mesh` 表达。显式批次适合大量顶点写回，避免每步重建 candidate。

## 尚未交付的契约

结构全量验证、保存和跨保存历史、资源诊断、Python wheel、wgpu 观察与统一验收入口均未实现。当前只在内存里创作和求值；资源描述指向磁盘文件，但 CPU 求值不读取纹理。`DocumentSession` 的旧公开 mutable API 仍供旧应用使用，SDK 不向其调用方导出该引用。SDK 路径使用单独 checkpoint history，不写旧 delta history。

逐方法迁移状态见 [SDK-COVERAGE.md](SDK-COVERAGE.md)。
