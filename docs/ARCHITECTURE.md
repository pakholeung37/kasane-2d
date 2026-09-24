# 当前架构

```text
Python wheel (kasane.Session) → kasane-sdk → kasane-core / kasane-project / kasane-moc3
             │                     │
             └── kasane.Observer → kasane-sdk-observe → kasane-preview
                                                        ↓
                                DrawableFrame → kasane-render → kasane-render-wgpu
                                                                      │
                                               PNG / focus crop
```

`kasane-sdk` 提供隔离编辑、版本检查、undo/redo、导入、工程保存和 MOC3 导出；Python wheel 暴露脚本入口。`kasane-sdk-observe` 从已发布或未发布的会话制作只读快照，复用 `kasane-preview` 的资源验证，并通过 `kasane-render-wgpu` 输出帧与诊断资料。

正式验收使用仓库外安装的 Python wheel、Rust 契约与 GPU 测试、官方/Purism Core 数值探针，以及仓库内固定的外部 GPU 参考图。参考图的输入哈希和来源记录在 `tests/fixtures/render_reference/`；[验证命令](VALIDATION.md)会检查输入与参考图身份。

Godot 编辑器、Viewer、demo 和 Rust GDExtension 已从当前应用路径移除。`gd-cubism` 仍用于独立的 Cubism benchmark，不参与 SDK 或 WGPU 构建。旧实现方案和里程碑记录见 [archive](archive/)。
