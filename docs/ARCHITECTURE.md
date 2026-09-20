# 当前原型代码与目标边界

本文仅记录仍存在的 C++ 实现。目标和验收以 [工程路线图](ROADMAP.md) 为准；现有接口和文件格式允许重构。

## 当前实现

- `kasane-core::Document` 保存源几何、稳定 ID、素材描述及变形关系，提供批量修改和现有 Rotation / 简单 Warp 求值，不依赖 Godot。
- `gd-kasane` 是 Editor 的内部 C++ 绑定代码。`KasaneDocumentBridge` 是 RefCounted 文档对象；预览节点、纹理存储和工程读写由独立对象负责。
- `KasaneMeshData` / `KasaneDeformerData` 是源对象句柄，校验拥有者、文档代次和对象 ID。Packed 数组为副本，修改后显式写回。
- 当前数据操作在 Godot 主线程进行。直接写入不创建历史；源数据快照和可选位置批次仍在 C++ 中，但不构成完整编辑器历史服务。
- 当前工程读写支持实验 JSON v1/v2。它不是新工程格式，也不是 MOC3 导出器。
- `KasaneMeshView` 是当前原生网格显示实现；公共 renderer 的目标见 [M4](milestones/M4-shared-renderer.md)。

## 已删除内容

旧预览应用、固定 Stage 工程、原型测试、GDScript 脚本宿主、Action 辅助脚本以及通用 addon 分发工具已经删除。不得继续把这些能力描述为当前可运行入口。

`gd-kasane` 的原生库只输出到模块 `build/bin/`。当前没有正式 Editor 应用，也没有独立供用户安装的 gd-kasane addon。

## 应用集成

[M5](milestones/M5-agent-editor.md) 在 `apps/editor/` 实现正式 Godot 应用，负责内部原生库加载、GDScript 编辑接口、脚本执行和桌面应用打包。用户直接启动 Editor，无需安装 Godot 或配置 GDExtension。

数据模型、求值与公共绘制代码放在可复用模块中；应用负责组合这些依赖。工程文件保存编辑源数据，MOC3 资源包交给既有运行时；两者与应用二进制分别交付。
