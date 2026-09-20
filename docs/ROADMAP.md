# Kasane Editor 工程路线图

更新：2026-09-20。本文定义工程目标与交付依赖；各里程碑独立成文。

## 1. 工程目标

**用 Godot 实现一个替代 Cubism Editor 建模工作的 Agent-first Live2D 编辑器，能够导入 MOC3、编辑模型、保存工程，并导出 Live2D 运行时可加载的 MOC3。**

Agent 通过进程内 GDScript 直接操作模型。人的界面用于查看、检查和辅助修改；脚本与界面使用同一份数据和编辑语义。

必须交付：

1. **Document 核心模型**：表达可编辑的素材、部件、网格、变形器、参数、绑定与关键形态，支持增删改查和求值。
2. **工程持久化**：完整保存、读取 Document 和素材引用，重开后继续编辑。
3. **MOC3 导入**：从运行模型创建真正可编辑的 Document，修改后可以再次导出。
4. **MOC3 导出**：从 Document 生成新的 MOC3 和加载所需的资源描述，在现有运行时中驱动、显示。
5. **公共 Godot renderer**：从 gd-cubism 提取实际绘制实现，编辑预览与 MOC3 播放共同使用。
6. **Godot Agent-first editor**：通过 GDScript 完成素材、mesh、deformer、parameter、keyform 的编辑，提供检查界面和执行反馈。
7. **Godot MOC3 viewer**：独立打开模型、调整参数和浏览结果，排在编辑器之后。

运行时基础已经存在：PurismCore + gd-cubism 能在 Godot 中加载和展示模型。后续复用这条路径，不重新立项实现另一种原生发布格式或播放器内核。

## 2. 最终交付产物

| 产物 | 使用方式 |
|---|---|
| Kasane Editor 桌面应用 | 用户直接启动，导入、建模、保存、导出；不需要安装 Godot、创建 Godot 项目或安装 addon |
| Kasane 工程文件与素材 | 保存完整可编辑数据，由 Editor 打开并继续编辑 |
| Live2D 运行模型包 | MOC3、纹理、model3.json，由现有 Live2D 运行时加载 |
| 独立 Viewer 桌面应用 | 直接打开运行模型并浏览，M6 交付 |

Godot 是应用实现技术。Document、求值、renderer 和 C++ 绑定是应用内部依赖，不作为要求用户安装的产品交付。Editor 的构建和打包负责携带原生库、加载配置和运行资源；不建设面向任意 Godot 项目的 addon 分发机制。

已建立 [Editor / Viewer 应用壳](../../apps/README.md)，可分别启动并展示检查面板、导航画布和功能占位；尚未接入 Document、文件读写、运行时和公共 renderer。应用壳不构成 M5/M6 验收；当前 C++ 库可构建也不代表建模功能已经可用。旧脚本宿主、Action、演示及原型测试已删除，按新 Document 接口实现应用能力。

## 3. 必须贯通的数据流

```text
PNG / 新建模型 ────────────────┐
                             ▼
MOC3 + 纹理 ──导入──→ Document ←──读写──→ Kasane 工程 + 素材
                             ↑
                   GDScript / 编辑界面
                             │
                  ┌──────────┴──────────┐
                  ▼                     ▼
               内存求值               MOC3 导出
                  │                     │
                  │               PurismCore 求值
                  └──────────┬──────────┘
                             ▼
                     公共 Godot renderer
```

- Document 是唯一可编辑模型；MOC3 和连续求值数组是派生结果。
- 工程保存保留编辑结构；MOC3 导出生成运行数据，两个动作独立。
- 导入必须恢复文件中支持的参数、绑定、关键形态和变形关系，不能只提取当前帧顶点。
- 导出必须从 Document 构建文件，不能复制输入 MOC3、修改几个字节或导出固定顶点帧来替代。
- 预览在内存中更新，不以每次编辑写出并重载 MOC3 为正常更新路径。

## 4. 当前代码的用途

以下是代码起点，不代表新里程碑已经通过验收。

| 现有代码 | 复用内容 | 必须补齐或改变的边界 |
|---|---|---|
| [kasane-core](../../modules/kasane-core/include/kasane/document.hpp) | Document、稳定 ID、几何校验、批量修改 | 参数、绑定、Keyform 和符合导出语义的模型；现有 Warp 不能直接视为 Cubism 等价实现 |
| [gd-kasane](../../modules/gd-kasane/src/document_bridge.hpp) | Godot 数据句柄、脚本绑定、源快照 | 数据绑定脱离场景节点；文件、素材、预览不再集中在 Bridge |
| [PurismCore 格式实现](../../modules/purism-core/src/moc3.h) | 文件结构、校验、加载与求值算法 | Document 与文件结构的双向映射、MOC3 写出；运行时内存布局不直接成为编辑模型 |
| [gd-cubism renderer](../../modules/gd-cubism/src/private/internal_cubism_renderer_2d.cpp) | 几何上传、材质、遮罩、顺序、批处理 | 绘制代码不再直接读取 CubismModel；去除对播放器 owner 的依赖 |
| [运行时对照](../../benchmarks/cubism-matrix/README.md) | Purism / 官方 Core 和 Godot / Native 对照设施 | 加入导出文件、导入往返和编辑预览的一致性验证 |

现有类名、目录、接口、demo 和实验工程格式均可重构。保留来源与版权说明。保留可复用代码和回归证据，不以维持旧架构为交付条件。旧工程若不提供迁移，必须明确拒绝，不能用新语义静默解释。

## 5. 本轮交付范围

终极目标是替代 Cubism Editor 的 Live2D 建模工作。本轮先交付下面明确的建模集合；完成它不等于已经覆盖 Cubism Editor 的全部功能。

| 范围 | 本轮必须交付 |
|---|---|
| 素材与画布 | PNG、多个纹理槽、画布尺寸与原点、单位换算、裁剪图层的位置 |
| 对象 | Part 组织、ArtMesh、Rotation / Warp deformer、稳定 ID 和引用 |
| 形态与驱动 | 参数范围和默认值、普通参数绑定、完整关键值组合、Keyform、区间内插值、嵌套变形 |
| 绘制 | 顺序、透明度、multiply / screen color、normal / additive / multiplicative 混合、普通及反向遮罩 |
| 编辑 | 上述对象的创建、读取、修改、删除、重绑定；批量几何和显式 Keyform 编辑 |
| 文件 | 本轮字段的工程保存读取、MOC3 导入、MOC3 导出与运行资源包 |

**本轮导出固定为 `csmMocVersion_50`（文件头版本值 5）的小端文件；导入覆盖 `csmMocVersion_33` 与 `csmMocVersion_50`（版本值 2、5）的小端文件。** 这对应本次核对的 Purism 测试模型 `3d8e869a678a1dac.moc3` 和现有 Mao 运行模型 `mao_pro.moc3` 的文件版本；版本标识见 [PurismCore 头文件](../../modules/purism-core/include/PurismCore.h)。选择现有验证资产的版本作为实施目标，不意味着这两个模型的全部特性已通过编辑器验收。Core ABI 版本与 MOC3 文件版本分别记录。

本轮不交付 BlendShape、Glue、Offscreen、参数循环、动作/表情/物理文件创作、Cubism `.cmo3` 工程互通、PSD 导入、自动网格生成和完整人工工具集。导入器遇到范围外语义必须报告并拒绝创建“可完整编辑”的工程；不得丢弃后返回成功。现有播放器对这些特性的能力不因编辑器范围而删除。

不假定 MOC3 包含原 Cubism 工程的全部编辑元数据。导入目标是文件中受支持模型语义的可编辑重建，不承诺恢复原始工程文件。范围扩展必须新增明确的字段、读写映射和回归用例，不能用“持续制作”“完善兼容”作为验收项。

## 6. 里程碑索引与依赖

旧 P1–P4 编排废止。M1 已完成并通过本机统一验收，包含嵌套变形、绘制/遮罩、运行包与 GPU 证据，见 [验收说明](M1-ACCEPTANCE.md)。M2 已完成目录工程、素材打包、跨进程往返及搬移后双 Core / GPU 验收，见 [M2 验收](milestones/M2-ACCEPTANCE.md)。M3–M6 的完整功能仍为**待实施、未验收**。按本轮先搭建所有应用壳的范围，M5/M6 的独立入口与最小界面骨架已提前建立，功能依赖与验收顺序保持不变。

| 里程碑 | 交付结果 | 依赖 |
|---|---|---|
| [M1 Document 与 MOC3 导出](milestones/M1-document-moc3.md) | **已验收**：代码建模、内存求值、MOC3/资源包导出与双 Core/GPU 对照 | 现有 core / 运行时 |
| [M2 工程持久化](milestones/M2-project-files.md) | **已验收**：保存、搬移、重开工程后继续修改和导出 | M1 数据契约；可随 M1 实施 |
| [M3 MOC3 导入与再编辑](milestones/M3-moc3-import.md) | 导入外部模型，编辑源结构，保存重开，再导出 | M1、M2 |
| [M4 公共 Godot renderer](milestones/M4-shared-renderer.md) | 编辑数据与运行模型使用同一份实际绘制实现 | 可先提取；验收使用 M1 求值输出 |
| [M5 Agent-first editor](milestones/M5-agent-editor.md) | 正式 Godot 应用内用 GDScript 完成建模、检查、保存、导入导出 | M1–M4 |
| [M6 MOC3 viewer](milestones/M6-viewer.md) | 独立 Godot 浏览应用，不加载编辑器数据与编辑设施 | M4、现有运行时；安排在 M5 后 |

M1 的最小导出应早于完整对象体系完成：先写出静态网格，再加入参数和变形，尽早验证模型设计能够生成有效 MOC3。不要等 UI 完成后才验证文件输出。

## 7. 实施纪律与完成判定

1. 每次实施只按对应里程碑的输入、输出、范围和验收表交付。修改范围时先修改对应文档，不自行把 MOC3 改成其他发布格式。
2. 复用 PurismCore 的格式知识、插值和变形算法，复用 gd-cubism 的绘制实现。新增重复实现必须说明现有代码无法复用的具体原因，并加入数值对照。
3. 不要求预先建立 Session 框架、任务调度平台、聊天面板或插件系统。临时选择、相机和预览值随应用管理；Agent 执行只需要最小可用入口。
4. 修改数据不强制成为 Command / Action；GDScript 与 UI 操作同一个 Document。可选撤销复用 Godot UndoRedo。
5. 所有里程碑遵守 [统一验收规则](VALIDATION.md)。代码存在、测试通过、实际模型通过三者分别记录。
6. 尚未验证的文件兼容性不标记为支持。必需的模型、SDK 或渲染环境缺失时，该项记为未验收，不能通过跳过测试完成里程碑。

最终应用验收必须同时完成两条路径：**PNG → 建模 → 保存重开 → 导出 → 运行**，以及 **外部 MOC3 → 导入 → 编辑 → 保存重开 → 再导出 → 运行**。
