# M1 核心重构与编辑契约

状态：已贯通 M1 对象、编辑、内存求值、MOC3 写出及运行包。完整验收入口与证据见 [M1 验收说明](M1-ACCEPTANCE.md)。

## 当前边界

```text
GDScript / 数据句柄
        ↓
KasaneDocumentBridge (RefCounted)
        ├─ Document：源数据、校验、编辑、revision
        └─ 临时 preview_values：不进入 Document / 工程文件
                  ↓
            evaluate_frame
                  ↓
            DrawableFrame
             ├─ 数据检查
             └─ KasaneDocumentPreview (Node2D) ← KasaneTextureStore

KasaneProjectIO ←→ Document
Document → kasane_moc3 → MOC3 字节与资源描述
```

- `kasane-core/include/kasane/model.hpp` 保存普通数据类型，`document.hpp` 保存编辑与身份管理接口，`evaluation.hpp` 定义统一求值输出。核心不依赖 Godot 或 Core 运行模型。
- `DrawableFrame` 使用运行坐标，包含画布、源 revision、实际参数值、几何/UV/索引和绘制字段。颜色、混合、遮罩、顺序、透明度均由源模型与 Keyform 决定。
- `evaluate_frame` 不调用 MOC3 编码器。编码器重新构建全部 Keyform 表，不能用当前预览几何替代形态。
- `KasaneDocumentBridge` 保留原类名，但已改为 `RefCounted`。其源数据接口不读取图片、不读写文件、不创建节点；Mesh/Deformer 句柄通过对象身份及 generation 访问数据。
- `KasaneDocumentPreview` 单独订阅源数据变化与预览值变化。纹理不足只令预览返回错误，不撤销已提交的源编辑。可以同时存在多个预览；销毁任一预览不关闭 Document。
- `KasaneTextureStore` 独立负责 Texture2D 资源。源模型只保存素材元数据，实际图片尺寸在加载/预览边界检查。
- `KasaneProjectIO` 独立保存/打开源快照，不加载纹理。打开失败不替换 Document；成功打开使旧数据句柄失效。完整工程搬移、素材打包等仍属于 M2。

## 编辑契约

Parameter 保存内部 ID、运行 ID、名称、范围、默认值和 decimal_places。当前支持每个 Binding 1–3 个普通非循环参数轴，驱动 Mesh 位置/绘制属性、Rotation/Warp 属性或 Part 顺序。

MeshBinding 显式保存轴顺序；每个 MeshKeyform 保存该轴顺序下的关键值组合及完整位置数组。提交时检查所有组合恰好出现一次，并规范化为轴 0 最快变化的顺序。重复轴、非递增关键值、缺形态、重复组合、未知参数、长度错误与非有限值均拒绝。一个目标不能被两个对应类型的 Binding 同时驱动。MeshBinding 保存 MeshKeyform；SceneBinding 的 target_id 指向 Part 或正式 Transform，保存 SceneKeyform。

三个操作分开：

1. `set_vertex_positions` 修改基础几何。已有绑定时不偷偷修改 Keyform。
2. `set_mesh_keyform` 修改显式关键值组合对应的完整形态。
3. `evaluate_frame(document, preview_values, frame)` 使用临时值，不修改源 revision 或 modified 状态。Godot 的 `set_preview_values` 替换整组临时覆盖，缺省参数使用默认值；返回请求值、钳制后的实际值和 clamped 标志。

`ChangeSet.object_ids` 扩展变化通知到参数、绑定、素材等对象；`mesh_ids` 保留受影响网格集合。

`references_to` 与 `erase_object` 要求先显式解绑，失败返回引用者。绑定可原子替换目标或参数轴。正式 Part 与 Transform 保存独立组织/变形父级；创建/替换校验类型、环和悬空引用。`replace_asset`、`replace_canvas` 提供素材元数据与画布修改。带 Keyform 的拓扑变更必须调用 `replace_mesh_with_keyforms`，提交所有新形态和每个新顶点对应的旧顶点 ID（新增顶点用空映射）；缺数据或引用错误时保持原 Document 不变。

## 算法复用与原型隔离

从 Purism 原有运行时提取普通数据输入的 `PurismKeyform.h`、`PurismMath.h`、`PurismDeformer.h`：

- 关键区间搜索与吸附阈值；
- 参数轴的组合索引和权重；
- 按相同顺序进行 float32 向量加权；
- Rotation 仿射、Warp 三角/quad 内插与近/远边界外推；
- 嵌套 Rotation 在父变形器下的方向求解。

Purism 原运行时和 Kasane 求值器调用同一份函数。提取不新增或改变 Core ABI 符号；CMake 安装和单文件 bundle 模板同时包含新头。改动位于 **Purism 子模块工作区**，提交时需要一并处理，不能只提交父仓库引用。

旧 Rotation/Warp 数据与算法移至 `legacy_deformer.hpp` / `legacy_deformers.cpp`。旧算法入口显式命名为 `evaluate_legacy_mesh`，仅供原型行为访问与回归；正式求值、Godot 正常预览和编码器均拒绝含旧 deformer 的模型。正式模型使用新的 Transform / RotationPose / SceneBinding。正式求值与导出使用该模型，legacy 类型不会被静默解释为新坐标域。

超出绑定关键值范围或被父级禁用的对象报告 enabled=false。Core 的禁用对象通道可能保留历史值，无状态求值清空其位置并使用默认绘制通道；对照跳过这些未定义历史通道。透明度零但 enabled=true 的对象仍有有效几何，供遮罩使用。消费者绘制时同时检查 visible 和透明度。

## Godot API 迁移

- Document 用 `KasaneDocumentBridge.new()` 创建，不再放入场景树。
- `initialize(id, canvas_size, origin, pixels_per_unit)` 后两个参数有默认值。
- `add_image_asset(id, name, source, width, height)` 接受元数据；Texture2D 交给独立 TextureStore。
- 原 `save_project/open_project` 移至 `KasaneProjectIO`，显式传 Document。
- 原预览操作移至 `KasaneDocumentPreview`，用 `set_document` 与 `set_texture_store` 连接。
- `get_frame`、`evaluate_mesh` 的位置明确使用运行坐标；只有预览适配器转换回 Godot 画布像素坐标。
- 原型快照版本升为 5，完整保存全部 M1 字段与 Keyform，不保存临时预览值。没有提供旧版本迁移，版本 1–4 明确拒绝。Document 数据接口 schema_version 为 3。

现有 MeshView 是 M1 临时绘制适配器，尚未变成 M4 公共 renderer；刷新时重建几何与遮罩，不宣称性能优化完成。当前已通过双路径 GPU 画面对照。相机缩放变化会重建遮罩分辨率；后续 M4 将复用 renderer 的 atlas/批处理设施。

## 可复现验证

从仓库根目录执行：

```sh
python3 tools/validate_core.py

# 安装 SCons 的 Python 环境；示例使用本地构建环境。
target/kasane/buildenv/bin/python -m SCons -C modules/gd-kasane platform=macos arch=arm64 target=template_debug -j8
python3 tools/validate_godot.py
```

- Core 报告：当前独立运行在 `target/kasane/core-regression/report.json`；历史整体验收记录在 `target/kasane/runs/<run-id>/core/report.json`。CTest 包括基础编辑、Keyform 数据契约、legacy 回归、两个 Core 的生成文件对照、Purism 单元、验证器负例与 C99 bundle；外部模型 conformance 需独立运行。
- 双 Core 测试覆盖三形态端点和中点、非对称 3×3、2×2×2、参数创建顺序与绑定轴顺序不同、预览范围钳制、单关键值轴、不可见绑定、修改中间形态、重绑定、完整拓扑替换和引用删除。
- `samples.json` 保留实际参数采样；`comparisons.json` 保留对象 ID、断言位置、expected/actual 和误差。
- `package/` 保留静态用例；`package-1d/`、`package-2d/`、`package-3d/` 提供相应的真实 MOC3、model3.json 和 PNG。
- Godot 报告：`target/kasane/godot-boundary/report.json`。59 项 headless 集成检查覆盖脱离场景树编辑、多个预览、资源错误、预览生命周期、源数据保存重开、失败原子性与句柄失效。GPU 另由 `validate_gpu.py` 独立执行。
- Purism v5 ABI 另通过当前新模型对照与 unit；其既有 stageplay 测试源使用 v6 专有接口，v5 构建失败，未计入通过项。v6 原有完整回归通过。
- 共享头的 C99 单文件 bundle 编译和运行通过。

### 已知编辑器导入问题

本机 Godot 4.7.2 mono / macOS arm64 在空缓存项目中首次扫描并加载扩展后，退出时出现 signal 11。将重构前 HEAD（`af498ffa`）的源码单独构建后，同样复现且引擎调用栈偏移一致；空项目不复现。因此它是本环境下既有问题，根因尚未确定，不能把编辑器首次导入计为通过。

预先写入 `.godot/extension_list.cfg`，让扩展在启动阶段加载，首次 editor import 返回 0；再次打开已经扫描过的项目也返回 0。headless 边界脚本使用前一种方式，headless 结果只覆盖数据与预览集成。复现日志分别位于 `target/kasane/godot-baseline-import/import.log` 和 `target/kasane/godot-boundary/import.log`；修复首次动态导入仍需单独跟进。

本轮补齐了正式 Rotation/Warp、Part、绘制属性/遮罩、编辑回归、通用发布器与 GPU 图像证据。M2 工程搬移、M3 导入、M4 公共 renderer 与 M5 编辑器功能另行验收。
