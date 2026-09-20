# M4：Rust 原生公共 Godot renderer

状态：实施中。随模块向 Rust 全面迁移，主导权收归 `kasane-godot`（纯 Rust GDExtension）；暂不强行将 C++ `gd-cubism` 接入该实现，`gd-cubism` 保留作为 GPU 像素级验收的独立外部基准（Ground Truth Oracle）。返回 [总路线图](../ROADMAP.md)。

## 1. 交付结果

在 `kasane-godot` 中定型纯 Rust 实现的 Godot 2D 渲染核心（基于成熟的 `KasaneDocumentPreview` 与 `KasaneMeshView`）。

1. **直接驱动编辑模型**：直接消费 `kasane-core::evaluate_frame()` 产出的内存数据（`DrawableFrame`），消除向 MOC3 编码的额外开销与延迟，实现高帧率交互预览。
2. **纯 Rust 统一渲染管线**：在 Rust 侧完整管理材质 Shader、Multiply / Screen 颜色、混合模式、正反向剪贴遮罩、动态顶点缓冲与图层排序。
3. **预留运行模型接入路径**：保留通过 C99 FFI 直接接入 `purism-core`（`csmUpdateModel`）的驱动接口，为 M6 独立 Viewer 提供不依赖 C++ GDExtension 的纯 Rust 闭环渲染能力。
4. **解耦 C++ 播放器**：暂不将 `gd-cubism` 作为该渲染器的被动消费者接入，避免跨 GDExtension 封包开销与双语言维护负担；`gd-cubism` 维持独立，作为成熟游戏播放框架与自动化验证裁判存在。

## 2. 架构与边界

```text
Document (编辑态) ──> kasane-core::evaluate_frame() ─┐
                                                     ├─> 统一绘制契约 (DrawableFrame) ─> Rust 公共 Renderer (kasane-godot)
MOC3 (运行态/Viewer) ──> purism-core (C FFI) ────────┘

[独立外部验证基准 (Oracle)]
gd-cubism (C++) ──> GPU 自动化像素比对 (validate_gpu.py) ──> 确保零色差、零形状误差
```

公共渲染器由 `kasane-godot` 独占实现与维护。公共层可以依赖 Godot API（`godot-rust`），但不得包含撤销重做（UndoRedo）、编辑器选择、历史记录或脚本宿主。

| 输入契约 | 内容 | 对应数据类型 |
|---|---|---|
| **Drawable 身份** | 稳定 ID；内部映射与外部覆盖层查找 | `String` / `id` |
| **几何** | 最终顶点位置、UV、稠密三角形索引 | `Vec<Vec2>` / `Vec<u32>` |
| **纹理** | 纹理引用、采样过滤与透明度约定 | `Texture2D` / `ImageAsset` |
| **外观** | 可见性、全局 Render Order、透明度、Multiply / Screen 颜色、三种混合 | `BlendMode`, `[f32; 4]`, `f32` |
| **遮罩** | 遮罩源 ID 集合、反向标记（Inverted Mask） | `Vec<String>`, `bool` |
| **更新类型** | 动态位置刷新（高频）、拓扑/材质变更（低频）、增删节点 | 区分处理，复用 GPU 资源 |

输入数据在提交当前帧有效，renderer 不保留上游可变裸指针。GPU 资源由 renderer 管理；移除对象与关闭模型时完全释放对应资源。

## 3. 实施与定型顺序

1. **固化动态顶点缓冲复用**：完善 `KasaneMeshView`，在拓扑不变时严格复用 `ArrayMesh` 与 RID，仅更新顶点缓冲（`FLAG_USE_DYNAMIC_UPDATE`），避免每帧分配销毁节点。
2. **定型材质与着色器规范**：提取标准 Live2D 着色器为公共 shader 资源，确保 `Multiply` / `Screen` 颜色公式、Normal / Additive / Multiplicative 混合模式行为与官方规范严格等价。
3. **完善剪贴遮罩管线**：维护多图层正向与反向遮罩隔离（SubViewport / Stencil 机制），避免跨帧残影与缩放锯齿。
4. **独立编辑覆盖层（Overlay Layer）**：高亮框、旋转手柄、网格控制点通过独立的 CanvasItem 覆盖层绘制，按稳定 ID 匹配当前几何，不修改底层模型材质或数据源。
5. **巩固自动化 GPU 门禁**：复用并维护 `tools/validate_gpu.py` 与 `tests/gpu_regression.gd`，持续以官方 Core + C++ `gd-cubism` 为外部基准做逐像素比对（84 项检查通过）。

## 4. 观察一致性

renderer 记录最后成功提交的输入版本和错误；截图与视觉比对必须等待对应帧的颜色与遮罩计算就绪。现有遮罩存在多视口更新依赖，必须等待对应 Viewport 渲染完成后再判定有效性。

定位与高亮使用独立覆盖层，按稳定 ID 找到求值几何，不修改模型材质或源数据。截图默认不含覆盖层。

## 5. 验收标准

遵守 [统一验收规则](../VALIDATION.md)。

| 用例 | 必须验证 |
|---|---|
| **GPU 视觉回归** | 以 `validate_gpu.py` 为基准，84 项 GPU 检查全部通过（单通道最大误差 $\le 0.001688$，全图均值差异为 0） |
| **外观组合** | 多纹理、排序、透明度、乘色/滤色、三种混合模式、普通/反向遮罩正常呈现 |
| **资源复用** | 固定拓扑连续更新位置，surface/RID 和 texture 不被逐帧重建，RSS 内存平稳收敛 |
| **结构变更** | 新增/删除网格、换拓扑、换遮罩后无残影、悬空节点与资源泄漏；关闭工程干净释放 |
| **双路径隔离** | 编辑预览不依赖 MOC3 导出与 C++ `gd-cubism`；运行播放不依赖编辑器设施 |
| **崩溃与边界安全** | 快速切换参数、窗口缩放、视口隐藏与重入借用均安全无 panic |

交付物包含：`kasane-godot` 内定型的公共渲染组件、Shader 资源、GPU 自动化比对套件与测试报告。
