# 当前架构与渲染边界


```text
Document (kasane-core) ──> DrawableFrame ──> ScenePlan (kasane-render)
                                              │
                                              v
                            Godot ViewLayout / 资源同步 ──> Editor 预览
                                              ^
                                     独立相机/viewport 更新

官方 Core + gd-cubism (C++) ──> 独立 GPU 对照基准
```

- [`kasane-core`](../modules/kasane-core/src/evaluation.rs) 管理可编辑模型并求出最终绘制帧，不依赖 Godot。
- [`kasane-render`](../modules/kasane-render/src/scene.rs) 校验输入并维护显式 target、target 内绘制顺序和原始 mask 源。缓存持有 ID、依赖、bounds 和已校验的静态 UV/索引 Arc，不保留已发布帧或动态顶点；旧 PreparedFrame 从同一逻辑描述降级生成，作为兼容入口。
- [`kasane-render-godot`](../modules/kasane-render-godot/src/backend.rs) 拥有节点、材质、纹理绑定和 viewport。物理 mask 按 consumer 和采样策略实例化，尺寸不是资源身份；相机更新只重新布局已有资源。模型更新通过材质值比较及实际节点身份/顺序比较跳过无变化的设置和重排。
- [`kasane-godot`](../modules/kasane-godot/src/document_preview.rs) 提供 GDExtension 数据绑定、纹理存储和预览生命周期/ready 观察。外部求值帧可直接提交给相同 backend，不需要构造 Document；提交时借用帧，相机更新不保存或复制整帧。
- [`kasane-preview`](../modules/kasane-preview/src/lib.rs) 负责项目资源验证，GPU 上传留在 Godot 适配器。
- [`gd-cubism`](../modules/gd-cubism/src/private/internal_cubism_renderer_2d.cpp) 仍是独立的 C++ 播放器，供 [GPU 回归](../tools/validate_gpu.py) 比对；不强制接入 Rust renderer。
- [`apps/editor/`](../apps/editor/) 和 [`apps/viewer/`](../apps/viewer/) 目前是应用壳。M5/M6 的正式功能和打包验收仍按各自里程碑执行。

历史 C++ 边界与迁移记录见 [`archive/`](archive/)；不能将其描述为当前 Rust 实现的接口。

第一阶段的实现范围、失效约束与后续事项见 [渲染边界方案](RENDER-BOUNDARY-PROPOSAL.md)。
