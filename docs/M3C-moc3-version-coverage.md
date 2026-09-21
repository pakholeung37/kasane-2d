# M3C：MOC3 版本 2–6 完整可编辑链路设计

状态：**设计完成，实施与验收待进行**。编写日期：2026-09-21。

本设计承接 [M3B](M3B-mao-editable-import.md) 与关联任务“检查并支持 moc3 version 3”。总体范围是文件头版本值 **2、3、4、5、6** 的小端模型；不把 `model3.json` 的 `Version: 3` 或 Core ABI 版本当成 MOC3 文件版本。未知的未来版本继续明确拒绝。

实施任务、依赖、交付物和逐阶段验收见 [实施计划](M3C-implementation-plan.md)。本文件定义贯穿各阶段的数据与兼容契约；两份文件共同约束实施。返回 [路线图](../ROADMAP.md)。

## 1. 目标与完成标准

交付完整链路：外部 MOC3/模型包 → 可编辑 Document → 编辑及预览 → 工程保存、脱离源包、重开 → 新 MOC3 → PurismCore 与官方 Core 加载、求值和渲染对照。

“支持”需要分别记录四项：结构识别、语义重建、编辑往返、真实模型验收。只接受版本号、默认帧相同、或者播放器能加载均不足以通过。实施期间按照模型实际使用的功能判断是否可导入，未实现的语义一次列全并阻止替换当前 Document。

本轮覆盖零三角形 ArtMesh、4.2 字段与 BlendShape、循环参数、5.0 BlendShape Glue、5.3 Offscreen 与扩展颜色/Alpha 混合。保留已有 Part、变形器、普通绑定、遮罩、Glue 和四类 BlendShape 能力。目标是保留文件中可表达的模型语义，不要求恢复 `.cmo3` 的历史或编辑元数据。

动作、表情、物理、Pose 等附件创作，大端输入，文件头版本 1，原版本 2/3/4 的专用导出器和 M6 独立 Viewer 产品不属于本轮交付。附件继续通过导入报告列明处理结果。M6 将复用本轮的帧契约，但不阻塞本轮 Editor 验收。

## 2. 源码基线及证据边界

主仓库 HEAD：`29685cb6adbb7070160229c2db846439c1f83c20`；PurismCore HEAD：`ad5127666ccffb34552d8e660852995858bee437`。编写时已有未提交的 ROADMAP、inspector 和 Rice 测试改动，本设计按当前工作区读取，不将它们视为本轮实施成果。

| 依据 | 当前事实与影响 |
|---|---|
| [inspector.rs](../../modules/kasane-moc3/src/inspector.rs) | 接受 2/3/5；固定 160 offsets；拒绝循环参数、BlendShape Glue 和 Offscreen；参数类型检查目前只在 `version >= 5` 执行 |
| [decoder.rs](../../modules/kasane-moc3/src/decoder.rs) | 普通关键形态颜色在 `ver < 5` 时跳过；需检查 4.2 的类型、颜色、增量池全部路径 |
| [geometry.rs](../../modules/kasane-core/src/geometry.rs) | `validate_render_mesh` 拒绝空三角形；[帧校验](../../modules/kasane-godot/src/render_frame_validation.rs)也复用它，故只修 importer 不够 |
| [types.rs](../../modules/kasane-core/src/types.rs) | 已有四类 BlendShape、Glue 与专用 GlueBinding；BlendMode 仅三种，尚无循环和 Offscreen 的完整模型表达 |
| [codec.rs](../../modules/kasane-project/src/codec.rs) | 当前写工程 v3，读取 v1–v3；新增语义必须升级格式，防止旧版本读入后丢字段 |
| [moc3.h](../../modules/purism-core/src/moc3.h) | 定义 V33/V42/V50/V53 分段；2–5 为 160 offsets，6 为 480；2–4 为 32 个 count，5–6 为 64 |
| [param.c](../../modules/purism-core/src/param.c)、[blendshape.c](../../modules/purism-core/src/blendshape.c)、[offscreen.c](../../modules/purism-core/src/offscreen.c) | 提供循环、Glue 增量、Offscreen 求值的本地实现依据；数值兼容仍需官方 Core 独立验证 |
| [官方 Framework OpenGL renderer](../../third_party/CubismSdkForNative-5-r.5/Framework/src/Rendering/OpenGL/CubismRenderer_OpenGLES2.cpp) | 有 Offscreen 父层关系、混合绘制顺序和合成路径；不能由仅输出 drawable 的探针替代 |

关联任务已定位 Hiyori 的 4 个 ArtMesh 有顶点但三角索引数为零，触发 `Triangle indices must be a non-empty multiple of three.`。本轮核实了拒绝路径，实施阶段 S0/S1 须重新输出具体对象、数组长度和失败回归。关联任务中的 Rice 双 Core 通过记录不外推到整个 version 3。

本轮直接读取的本地样本清单如下；这些是结构信息，不是本轮运行验收结果。路径统一位于 `third_party/CubismSdkForNative-5-r.5/Samples/Resources/<名称>/<名称>.moc3`。

| 样本 | 文件头版本 | Mesh / Offscreen 数 | SHA-256 |
|---|---:|---:|---|
| Rice | 3 | 178 / 0 | `be56f2d656d279d8db9fe302ec744fa5992ff884b2ebb233f202772d5dd7e4dc` |
| Hiyori | 3 | 134 / 0 | `0323f5f377b9afef54318be09e9b4eea2a8cb266190ff9d2f843f95ecb77716c` |
| Mark | 3 | 30 / 0 | `f6351429bd3dd655ac95d1027e463ae415e99e2b1b43431a9662fcd316373e9e` |
| Ren | 6 | 198 / 24 | `a9ccf7d6b2f4f16c8b13da29b9b4f34dc865ba9c8ad88a0388530e7f2d6cc533` |

version 2 沿用 Purism fixture；version 5 沿用原始 Mao 和现有外部 fixture。version 4 的真实外部模型、实际包含循环/BlendShape Glue 的外部资产覆盖仍待补齐；Ren 也不能代表全部混合模式。缺样本不阻止实现和构造测试，但对应真实模型验收保持 `not_run`。

## 3. 版本与能力矩阵

以下版本名以仓库内 [PurismCore.h](../../modules/purism-core/include/PurismCore.h) 为准；这是本地固定 SDK 基线，不声明未来版本兼容性。

| 文件头值 | 版本名 | offsets / count 项数 | 本阶段重点 |
|---:|---|---|---|
| 2 | MOC 3.3 | 160 / 32 | 基础语义回归、循环及空拓扑边界；普通 Glue 原已存在 |
| 3 | MOC 4.0–4.1 | 160 / 32 | 沿用 3.3 section 集；完成 Hiyori、Rice、Mark 全链路 |
| 4 | MOC 4.2 | 160 / 32 | 参数类型/键查询表、普通乘色/屏色、Warp/Mesh BlendShape 与约束 |
| 5 | MOC 5.0–5.2 | 160 / 64 | 独立增量关键形态颜色偏移、Part/Rotation/Glue BlendShape；前两者已有基础 |
| 6 | MOC 5.3 | 480 / 64 | Offscreen、Part 到 Offscreen 关键形态索引、Offscreen BlendShape、Mesh/Offscreen 扩展混合 |

循环参数和零三角形是跨版本语义，不归因于某个新增版本。能力表应拆为 `format_capabilities`（格式能表达）与 `implemented_capabilities`（本应用已完成的链路），另用验收矩阵记录样本证据；不能用一张布尔表同时代表三者。

建议内部 `MocLayout` 集中定义 offset 容量、有效 section 集、count 长度和功能门槛。decoder/encoder 不继续散落 `ver >= 5`；4.2 的字段由 V42 能力控制，5.0 的字段由 V50 能力控制。名称是设计建议，尚非现有 API。

## 4. 整体架构

```text
外部 bytes → 有界结构检查 → 版本布局/功能清单 → 可编辑能力检查
                                                 │
                                                 ▼
                工程读写 ←── Document / 编辑 API / 事务
                                  │              │
                                  ▼              ▼
                         内存求值 RenderFrame   导出预检
                                  │              │
                         渲染计划/离屏合成      5.0 或 5.3 encoder
                                  │              │
                              Editor         双 Core + 独立 GPU 对照
```

Document 仍是唯一编辑真值。原始 MOC3 可记录路径、hash 和来源版本，但不作为新增功能的隐藏求值器或导出副本。不通过烘焙顶点、降级混合或删除零三角形对象取得导入成功。

### 4.1 有界检查和导入事务

1. 校验 magic、版本、端序、offset 表长度及 count 表窗口。未知版本返回版本错误；有效但尚未实现的功能返回功能错误；损坏数据返回结构错误。
2. 用 checked arithmetic 验证乘加、长度转换、对齐、有效 section 窗口、对象索引、键窗口及关系环，再遍历功能引用。零长池与“字段在此版本不存在”分别表达，不能将 offset=0 当成有效数据起点。
3. 保留 runtime 指针占位和磁盘数据字段的差异；不将指针位宽当作磁盘元素宽度。ABI 与格式版本独立记录。
4. 无 PurismCore 构建也必须安全；缺少可选 C 一致性检查不能让越界引用进入 decoder。C 检查作为附加基准，不替代 Rust 的有界访问。
5. 诊断包含版本、feature、对象 runtime ID/源索引、section/字段、原因。功能诊断可聚合；一旦结构不安全，不继续解引用以收集更多信息。
6. 所有模型在候选 Document 完整验证后原子替换当前工程；纹理、工程写入或导出失败不得留下部分提交。

### 4.2 零三角形 Mesh

在 Document 中保留 Mesh、顶点/UV、普通/增量 keyform、mask 引用、Glue 引用与 ID。零三角形表示无光栅化贡献，仍可参与几何求值、引用和编辑。不能删除对象并重排原模型索引，也不能生成虚构三角形。

拆开“几何结构有效”与“可提交绘制”：三角形数组可空，非空长度必须为 3 的倍数；保持索引、有限数及 UV 数量检查。当前少于三个顶点的拒绝规则也需审计，S0 对 Core 有效的零/一/二顶点无面对象取证，S1 明确其可表示条件；不以修复 Hiyori 为由跳过检查。

帧中保留逻辑 drawable 身份；renderer 跳过无面 draw call 并清理旧 GPU 几何，mask pass 仍按空覆盖贡献处理。被其他对象作为普通/反向遮罩引用时，以官方结果验收，不因空源而删除 mask 引用。Bounds、拾取与 overlay 必须处理空集合，避免 NaN、除零或旧帧残留。恢复三角形后能重新显示。

### 4.3 4.2 颜色及 BlendShape

复用 M3B 的稳定 ID、键表、约束和 typed delta 架构。补齐 V42 参数类型和归属、普通颜色窗口、Warp/Mesh 增量形态；V50 专属的逐关键形态颜色偏移及 Part/Rotation/Glue 表只在该版本存在时读。

导入普通外观与增量通道必须分别映射：4.2 不存在的字段不能从填零的 offsets 中读取，也不能虚构 5.0 增量颜色。导出 5.0 时为缺失的增量通道写入语义中性值/缺省标记。普通颜色无池、合法缺省、负哨兵和非法越界分别验证；所有现存按 `ver >= 5` 控制的解码和 inspector 路径都需逐项审计。

复用基准键、非基准增量和约束 `min` 合成规则，保留共享表、多绑定及源顺序。新增用例覆盖基准位于中间、负增量、父颜色继承和 A→B→A，不用单个真实模型替代功能用例。

### 4.4 循环参数

`Parameter` 新增显式 repeat 语义，默认 false。输入值和实际求值值分开报告。普通参数保持钳制；循环参数按固定 Core 基线归一化。Purism 当前使用 `min + fract((input-min)/(max-min)) * (max-min)`，fract 为减 floor，故精确 max 回到 min；这个边界必须由官方 Core 实测确认后固化。

循环参数要求有限且 `max > min`；非法范围在创建、导入和编辑均拒绝。普通绑定、BlendShape 键查找、约束读取使用同一实际值；不能只给滑块做取模。预览 API 允许跨周期输入，显示输入与实际值，保存的是模型定义而非临时预览状态。测试负向、多周期、精确端点和接缝两侧，并验证关键表接缝行为而非假定取模即可获得正确插值。

### 4.5 BlendShape Glue

为已有 `BlendShapeTargetKind` 增加 Glue，delta 增加强度通道；绑定引用 Glue 的稳定 ID，复用现有键表与约束，不另外创建一套参数系统。

流程为普通强度求值 → Glue 增量叠加与规定钳制 → 两端变形完成 → 按既有 Glue 顺序修改坐标。Purism `psm__blend_glues` 将增量结果限制到 `[0,1]`；普通 Glue 不因此改变原有强度规则。多条绑定、约束共享、负增量、钳制上下界、两端非对称权重均需官方对照。

删除 Glue/参数/键表、拓扑替换、事务、快照和 Undo/Redo 覆盖新增引用。导出重建 `bs_glue_src` 与强度池窗口，保留基准槽；不能借用 mesh keyform 或复制原始窗口。

### 4.6 Offscreen、扩展混合和帧契约

**Document：**新增独立 Offscreen 对象，含稳定 ID、owner Part、源顺序、mask 引用及反向标志、颜色/Alpha 混合状态、普通 opacity/颜色形态和增量绑定。源文件没有可用 ID 时生成确定性 ID，并持久化映射。Part 普通 keyform 到 Offscreen keyform 的索引关系须显式保留，包括合法负哨兵；不得假定与 Part 关键形态一一连续对齐。普通 Offscreen 求值复用 owner Part 的轴与键选择，不新增会与 owner 脱节的独立时间线。

**引用约束：**校验 owner、Part→Offscreen、关键形态、mask 和 BlendShape target；所有关系遵循固定格式的有效性规则。通过显式引用检查和排序派生父离屏关系，拒绝不可调度的依赖。重父级操作须更新合成关系或原子拒绝。禁止简单按 Mesh 列表连续片段推断归属。

**混合表示：**从现有三种 `BlendMode` 扩展为可无损表达 Core 颜色/Alpha 模式的类型，保留原三种模式的兼容含义。对固定 SDK 枚举列全编码、公式和可用组合；不能将未知值降级 Normal，也不能把 AddCompatible 与新 Add 未经验证合并。读写预检与 shader 调度使用同一模式映射表。

**求值：**新增 Offscreen 普通/增量 opacity、multiply/screen、enable 和混合排序结果；普通求值、增量叠加、父继承、禁用表面的最终 opacity 等顺序逐项对照 `update.c`、`offscreen.c` 和官方 Core。Offscreen 没有网格顶点，不伪装成普通 Mesh。

**帧输出：**建议将当前 DrawableFrame 演进为 `RenderFrame { canvas, drawables, offscreens, ordered_objects }`，绘制对象为 typed Mesh/Offscreen 引用。保留按稳定 ID 比较的映射。core 输出逻辑对象、归属与顺序；Godot 层编制开始表面、绘制、结束及合成的渲染计划。无 Offscreen 的旧模型保持单通路结果。Viewer 将来从 Core 转换为同一帧，不再另造合成语义。

**renderer：**实现父子离屏表面、透明背景、最终合成、Mesh 与 Offscreen 遮罩、反向遮罩及目标颜色读取。需要 destination texture 的模式使用独立目标/乒乓缓冲，不能同时读写同一 attachment。锁定预乘 Alpha、色彩空间、viewport/camera、清屏和采样规则；计算顺序不得把父 opacity 重复应用于每个子 Mesh。

**缓存：**池化离屏纹理/FBO，尺寸与相机变化时正确重建；禁用/启用、空组、结构变更、遮罩变化清理旧内容；重复稳定帧不持续分配。资源预算和最大表面尺寸在 S5 测量后固定，超限给出明确错误，不静默丢组或降级混合。

**独立参考：**现有 gd-cubism 对扩展混合的兼容映射不能作为 5.3 的唯一 GPU oracle。S0 先验证本地官方 Framework 的 Native 路径可输出正确离屏截图；S6 以官方 Core + 官方 Framework 为主参考，gd-cubism 继续承担旧能力回归。参考路径能力不足时记 `not_run`，不能两个近似 renderer 相互证明。

### 4.7 工程格式、编辑入口与导出策略

工程格式预留 **v4**，统一保存 repeat、Glue 增量、Offscreen、混合模式及关键形态引用。新 reader 明确迁移 v1–v3：repeat=false、无新增集合、原混合语义不变；旧 reader 应拒绝 v4。S3 引入格式版本和新增字段契约，后续阶段补齐功能时不改变同一字段的意义。处于过渡阶段的 reader 必须检测尚未实现的非空字段并拒绝，不能反序列化时忽略。若后续设计发生不兼容调整则再升级版本。

每个新增模型能力随阶段交付 GDScript CRUD/查询、检查界面、快照/事务、导入摘要、保存重开及导出预检，不能把“可编辑”推迟到最终 UI 补课。预览参数与普通/增量关键形态编辑继续分别操作。工程纹理继续可搬移，运行无需读取源 MOC3。

导出选项建议 `Auto | Moc50 | Moc53`：

- Auto：Document 无 5.3 必需语义时写版本 5；存在 Offscreen 或 5.3 专属混合时写版本 6。来源版本仅作提示，不主导目标版本。
- 强制 Moc50：预检列出所有不能表达的对象并拒绝；不烘焙、不扁平化、不改混合模式。
- 强制 Moc53：按 480 offsets、64 count 和 V53 schema 从 Document 完整构建；无扩展模型也必须正确加载。
- 本轮不提供写回 2/3/4 的选项。输入旧版本转换到 5 是语义往返，非逐字节往返。
- 文件和资源包写出前完成容量、引用、section/count 及目标 Core 能力检查，失败保留旧产物。

## 5. 验收、阻塞与发布规则

遵循 [统一验收](../VALIDATION.md) 的数值、像素及失败原子性阈值。循环参数是其“参数超范围钳制”的显式例外：S3 同步修订规则，循环用归一化后实际值比较，非循环继续钳制。

数值路径覆盖原文件双 Core、Document、工程重开、再导出双 Core。对齐 ID，比较结构、键表/约束、拓扑/UV、位置、颜色、opacity、顺序、mask，以及新增 Offscreen 归属、启用、关键形态和混合状态。项目重开比较完整持久化语义，不能只比截图。

GPU 使用统一阈值，并增加嵌套离屏、半透明、每种受支持混合、普通/反向遮罩、同帧多表面、相机和尺寸变化、连续参数 A→B→A、空/禁用表面。全图和关键局部均比较，保留透明与不透明背景测试。

功能测试可以使用构造文件，但真实外部模型不能由本仓库 encoder 自产，也不能只改已有文件的版本字节冒充。报告必须将构造、真实及未运行项目分开；缺少 version 4 或特定功能的真实资产时，不宣称全版本完整验收。

每阶段独立提交、独立报告。新能力只有完整阶段门禁通过才进入公开可编辑支持集合；低阶段可先交付，高阶段继续明确拒绝。不能撤回其他已验证能力来让新阶段测试通过。

## 6. 待实施阶段解决的具体问题

| 问题 | 解决阶段 | 不满足时的处理 |
|---|---|---|
| version 4、循环及 BlendShape Glue 的真实外部覆盖不足 | S0 获取清单，S2/S3/S4 补齐 | 实现可继续，真实验收不得通过 |
| 零/少顶点无面对象、空 mask 的 Core 有效性 | S0/S1 最小文件及外部对象对照 | 按已证实语义定义边界，未知输入明确拒绝 |
| 4.2 与 5.0 的颜色池、缺省和增量字段差异 | S2 固定字段映射与逐通道对照 | 不借填零 offsets 读取后续版本字段 |
| 循环 max 端点、接缝及约束行为 | S3 双 Core 测量 | 出现差异先定位，不放宽误差 |
| Offscreen owner/Part key 索引/排序和颜色继承 | S5 源码映射与官方 Core 对照 | 无数值证据不进入 renderer 发布 |
| 官方 5.3 GPU 参考可用性及全部模式公式 | S0 验证参考，S6 枚举并验收 | 保持未验收，不以 gd-cubism 降级画面替代 |
| 新帧与渲染资源开销 | S5 建基线，S6 测量 | 固定预算和超限错误，再完成 S7 |

此清单是有明确出口的实施任务，不是要求在动工前再次确认范围。
