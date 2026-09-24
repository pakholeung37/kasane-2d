# 当前架构

```text
Python wheel (kasane.Session) → kasane-sdk → kasane-core / kasane-project / kasane-moc3
             │                     │
             └── kasane.Observer → kasane-sdk-observe → kasane-project
                                                        ↓
                                DrawableFrame → kasane-render → kasane-render-wgpu
                                                                      │
                                               PNG / focus crop
```

`kasane-sdk` 提供隔离编辑、版本检查、undo/redo、导入、工程保存和 MOC3 导出；Python wheel 暴露脚本入口。`kasane-sdk-observe` 从已发布或未发布的会话制作只读快照，通过 `kasane-project` 读取并验证图像资源，再由 `kasane-render-wgpu` 输出帧与诊断资料。

## SDK 与 Python 包的职责

`kasane-python` 是一个发行包，内部同时包含 PyO3 Rust 扩展和纯 Python 代码；它不是与 `kasane-sdk` 并列的第二套工程内核。区分代码位置时看**谁拥有规则、谁会使用规则**，不以实现语言或文件长度决定。

| 位置 | 负责什么 | 本仓库实例 |
| --- | --- | --- |
| `kasane-core` / `kasane-project` 等底层 crate | 文档数据、求值及格式规则 | mesh、binding、画布与工程格式 |
| `kasane-sdk`（Rust） | 跨语言一致的编辑契约：事务、依赖验证、版本、历史、导入导出 | `EditSession::replace_topology` 在一次编辑中检查并替换 mesh 与关联对象 |
| `kasane-python/src`（PyO3） | 将 Python 值转成 Rust 类型，调用 Rust SDK，转换错误与结果；不维护另一套文档状态 | `NativeSession`、`NativeEdit` |
| `kasane-python/python/kasane` | 面向 Python 用户的类型、快照、上下文管理器、路径习惯和工作流 | `Session`、`Edit`、批量观察与报告；建模方法包装 Rust API |
| `docs/experiments/` | 依赖特定模型、映射或尚未验证为通用能力的一次性实验 | Shirousagi 的参考位移投影、头部图层选择和视觉比较 |

新增能力先确定规则所有权，再选择语言。决定模型变化、求值或形变保持的正式算法由 Rust 体系实现，即使当前只有 Python 调用者；文档不变量归 core，事务与建模操作归 SDK，持久化归 project。Python 可组合已有操作完成批处理、报告与脚本便利语法，但组合中若产生新的建模规则，也应交由 Rust 维护。尚未稳定的模型专属算法保留在实验中。

`rectangle_grid_geometry` 和 `AuthoringSession::remesh_rectangle_grid` 由 `kasane-sdk/src/geometry.rs` 实现。Rust 统一负责规则网格生成、保留角点 ID、三角形插值、绑定与 BlendShape 迁移及 glue 收集，并通过 `replace_topology` 原子提交。Python 同名 API 只负责语言类型校验、数据和异常适配。Rust 测试验证整个重建曲面的求值与历史，Python wheel 测试验证入口以及绑定、BlendShape、glue 和保存重开。参考模型位移投影的网格外回退策略尚未通用化，因此仍属于实验脚本。

`python/kasane/__init__.py` 仅维护稳定的公开导出和公开类的兼容模块路径。`_types.py` 定义记录与类型别名，`_conversion.py` 转换 native tuple，`_edit.py` 包装编辑上下文，`_session.py` 包装会话并维护脚本运行器的弱引用注册表，`_observe.py` 负责观察与报告。内部模块直接依赖类型和适配模块，不通过包入口回引；`geometry.py` 与 `_mesh_edit.py` 只适配 Rust 建模能力。拆分后保持 `import kasane` 和已有记录的 pickle 路径，通过安装 wheel 后的 CPU、GPU 和公开类型兼容测试验证。

正式验收使用仓库外安装的 Python wheel、Rust 契约与 GPU 测试、官方/Purism Core 数值探针，以及仓库内固定的外部 GPU 参考图。参考图的输入哈希和来源记录在 `tests/fixtures/render_reference/`；[验证命令](VALIDATION.md)会检查输入与参考图身份。

Godot 编辑器、Viewer、demo 和 Rust GDExtension 已从当前应用路径移除。`gd-cubism` 仍用于独立的 Cubism benchmark，不参与 SDK 或 WGPU 构建。旧实现方案和里程碑记录见 [archive](archive/)。
