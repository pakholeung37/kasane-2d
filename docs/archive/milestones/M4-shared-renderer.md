# M4：Rust 原生公共 Godot renderer

状态：已实施并通过本机验收，见 [M4 验收记录](M4-ACCEPTANCE.md)。渲染核心由 `kasane-godot`（Rust GDExtension）维护；C++ `gd-cubism` 保留为独立 GPU 对照基准。返回 [总路线图](../ROADMAP.md)。

## 1. 交付结果

在 `kasane-godot` 中定型纯 Rust 实现的 Godot 2D 渲染核心（基于成熟的 `KasaneDocumentPreview` 与 `KasaneMeshView`）。

1. **直接驱动编辑模型**：直接消费 `kasane-core::evaluate_frame()` 产出的内存数据（`DrawableFrame`），消除向 MOC3 编码的额外开销与延迟，实现高帧率交互预览。
2. **纯 Rust 统一渲染管线**：在 Rust 侧完整管理材质 Shader、Multiply / Screen 颜色、混合模式、正反向剪贴遮罩、动态顶点缓冲与图层排序。
3. **预留运行模型接入路径**：绘制核心接收已求值的 `DrawableFrame` 和纹理表，不要求构造 Document。M6 再实现 `purism-core` C99 FFI、运行帧转换和 Viewer；M4 不将 FFI 接入本阶段验收。
4. **解耦 C++ 播放器**：不将 `gd-cubism` 作为该渲染器的消费者接入。`gd-cubism` 维持独立，作为现有游戏播放框架与 GPU 像素对照基准。

## 2. 架构与边界

```text
Document (编辑态) ──> kasane-core::evaluate_frame() ─┐
                                                     ├─> 统一绘制契约 (DrawableFrame) ─> Rust 公共 Renderer (kasane-godot)
MOC3 (M6 Viewer) ──> purism-core (C FFI) ──> 帧转换 ┘

[独立外部验证基准 (Oracle)]
gd-cubism (C++) ──> GPU 自动化像素比对 (validate_gpu.py) ──> 按统一阈值验收
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

`get_observation_state()` 返回递增的 `submission_id` 与源 revision；绘制前记录对应提交及目标 Viewport RID，绘制后只确认该提交，含遮罩的帧等待目标视口完成两次绘制。信号随节点入树/出树连接与断开，重新入树会重新等待绘制。新提交与失败结果都会使旧帧不能冒充当前可观察结果。

截图使用可见窗口，或尺寸至少为 2×2 的 SubViewport，后者必须设置 `UPDATE_ALWAYS` 或显式请求 `UPDATE_ONCE`；含遮罩时需要两次单次请求。就绪判断查询 RenderingServer 的实际更新模式，不能依赖 SubViewport 缓存的 `ONCE` 属性。`UPDATE_DISABLED` 不推进完成状态；`UPDATE_WHEN_VISIBLE` / `UPDATE_WHEN_PARENT_VISIBLE` 的实际更新无法由该接口可靠确认，因此新提交保持未就绪，调用方需要切换为显式更新模式。

外部 `submit_frame()` 与 Document 帧共用提交前校验：画布、完整三角形、索引范围、UV 数量、有限坐标与颜色、唯一 ID、遮罩引用和纹理。失败返回错误状态，不将失败帧缓存为可重放输入，也不报告旧图像就绪。模型节点在独立子容器中排序，刷新不会改变模型与选择覆盖层的前后关系。

定位与高亮使用独立覆盖层，按稳定 ID 找到求值几何，不修改模型材质或源数据。截图默认不含覆盖层。

## 5. 验收标准

遵守 [统一验收规则](../VALIDATION.md)。

| 用例 | 必须验证 |
|---|---|
| **GPU 视觉回归** | 以 `validate_gpu.py` 为基准，84 项 GPU 检查全部通过；实际误差与阈值记录在报告中 |
| **外观组合** | 多纹理、排序、透明度、乘色/滤色、三种混合模式、普通/反向遮罩正常呈现 |
| **资源复用** | 固定拓扑连续更新位置，surface/RID 和 texture 不被逐帧重建，RSS 内存平稳收敛 |
| **结构变更** | 新增/删除网格、换拓扑、换遮罩后无残影、悬空节点与资源泄漏；关闭工程干净释放 |
| **双路径隔离** | 编辑预览不依赖 MOC3 导出与 C++ `gd-cubism`；绘制核心可直接接受外部求值帧，M6 再验收运行模型接入 |
| **崩溃与边界安全** | 快速切换参数、窗口缩放、视口隐藏与重入借用均安全无 panic |

交付物包含：`kasane-godot` 内定型的公共渲染组件、Shader 资源、GPU 自动化比对套件与测试报告。

从仓库根目录运行 `cargo build --release -p kasane-godot --locked`，再运行 `python3 tools/validate_m4.py --library target/release/libkasane_godot.dylib`；结果写入 `target/kasane/m4/report.json`。验收入口同时运行 Rust 帧校验单元测试和真实 GPU 的观察状态/覆盖层回归。
