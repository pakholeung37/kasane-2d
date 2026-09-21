# M3C 分阶段实施与验收计划

状态：**S0、S1 已通过，S2 待进入**。日期：2026-09-21。

范围、源码证据和数据契约以 [总体设计](M3C-moc3-version-coverage.md) 为准。此前 Rice 改动与 M3B 完成记录属于基线，不代表本计划已通过。实施时逐阶段将状态更新为 `in_progress / passed / failed / not_run` 并链接实际报告。

## 1. 依赖和交付顺序

```text
S0 基线/样本/参考路径
 └─ S1 安全布局与 version 3 / 无面网格
     └─ S2 version 4 的完整字段与往返
         └─ S3 循环参数与工程 v4
             └─ S4 BlendShape Glue / version 5 补齐
                 └─ S5 version 6 数据、求值与编码
                     └─ S6 Offscreen / 扩展混合渲染
                         └─ S7 全版本整体验收与发布
```

默认依次实施，阶段内部可先推进不依赖缺失外部资产的代码与构造测试；门禁未通过时不得把该能力标为已验收。S0 即启动 4.2 素材与 5.3 GPU 基准准备，避免最后才发现无对照条件。每阶段按“数据/校验 → 读写/求值 → 编辑/持久化 → 验收”交付可审查改动，避免一次提交所有版本。

| 阶段 | 规模/主要风险 | 对外可交付结果 |
|---|---|---|
| S0 | 小；环境和外部资产 | 有版本/feature/hash 的可复现基线 |
| S1 | 中；共用几何与 mask 路径 | Hiyori/Rice/Mark 所用语义可编辑往返 |
| S2 | 中；跨版本字段误读 | version 4 普通颜色及 Warp/Mesh BlendShape |
| S3 | 中；接缝和持久化兼容 | 循环参数及工程 v4 |
| S4 | 中；增量顺序、引用与强度钳制 | BlendShape Glue 完整编辑链路 |
| S5 | 大；Part/Offscreen 映射及新编码 | 5.3 数据与数值链路，预览尚不宣称完成 |
| S6 | 大；GPU 依赖、Alpha 和离屏顺序 | 5.3 完整预览及渲染对照 |
| S7 | 中；组合回归与打包环境 | 有证据的版本 2–6 支持矩阵 |

规模用于拆分工作，不是工期承诺。S5 建议分数据模型、decoder/evaluation、encoder/project 三个依次可审查的提交；S6 分渲染计划、离屏/遮罩、模式公式、缓存/回归四个提交。

## 2. S0：建立失败基线和参考环境

**输入：**当前工作区、PurismCore、官方 Native SDK、M3B 验收工具和本地样本。

**任务：**

1. 记录主仓库与子模块 revision、已有未提交改动、编译选项、Core ABI、Godot/平台，先运行已有目标测试确认基线。
2. 建立样本 manifest：入口、文件和纹理 hash、文件头版本、模型 counts、实际 feature、所需验收案例。完整扫描版本与功能，重复文件按 hash 合并覆盖证据。
3. 为 Hiyori 保存原始失败报告，列出 4 个无面 Mesh 的 ID、顶点/索引数、keyform、mask/Glue 引用。用最小用例核对 0/1/2 顶点无面的 Core 接受范围。
4. 获取或定位真实 4.2、循环参数、BlendShape Glue 样本；保存许可/本地路径约定。不提交不可分发模型，不以改版本头生成的文件替代。
5. 验证官方 Core 探针的 5.3 API 与 build flags，补 Offscreen 数值字段；确认官方 Framework Native renderer 可对 Ren 输出带离屏的参考截图。
6. 规划统一 `tools/validate_m3c.py` 和 manifest/report schema。先实现基线模式，使缺少必需样本/SDK返回明确 `not_run` 和非零总退出状态。

**修改位置：**`tools/`、`tools/probes/`、`benchmarks/cubism-matrix/`、MOC3 集成测试及本计划的报告链接。

**产物：**baseline manifest、Hiyori 失败证据、Core/GPU 参考能力表、待补资产清单。

**门禁：**所有已有样本可识别来源；能重现已知拒绝；参考与工具缺口被列明。缺资产允许进入实现，但依赖它的验收保持未完成。

## 3. S1：安全布局、零三角形和 version 3

**前置：**S0 的结构盘点和已知失败证据。

**任务：**

1. 在 moc3 模块集中版本布局定义，加入 4/6 的结构识别能力，区分检查 API 与导入能力。4/6 的未实现功能在导入层明确拒绝，不能因可检查就开放完整导入。
2. 对 160/480 offsets、32/64 counts 和各版本有效 sections 建边界检查；修复 feature 遍历早于安全验证、可选 Core 缺席时缺少保障的路径。
3. 拆分 core 几何结构检查与 GPU 绘制资格，保留合法无面网格。同步 document、frame preflight、预览、mask atlas、bounds、overlay 和导出器。
4. 检查 decoder 是否先验证原始索引长度再组装三角形，避免截掉末尾 1/2 个索引后误判成功。无面对象不能在 import/export 中被过滤。
5. 为零三角形对象提供脚本查询/顶点编辑/补三角形；保存重开、撤销恢复、导出对象 ID 与引用一致。
6. Rice、Hiyori、Mark 使用统一参数采样跑原文件→Document→工程→导出的数值及应用/GPU 流程。

**修改位置：**`kasane-moc3/src/{inspector,decoder,encoder,layout,schema}.rs`、`kasane-core/src/{geometry,document,evaluation}.rs`、`kasane-godot/src/{render_frame_validation,document_preview,selection_overlay}.rs`。

**必需测试：**无面对象是 Glue 端点/遮罩源/被遮罩对象；无面→有面→无面的连续帧；空数组 bounds；未知版本、截断 480 表、溢出/负数/悬空索引；有/无 PurismCore 两种构建。

**门禁：**三份真实 version 3 模型所用功能全链路通过，Hiyori 四个对象保留；旧 2/5 回归通过；循环等尚未实现能力仍准确拒绝。此时仅宣称已验收模型与功能，不泛称全部 version 3 完整支持。

## 4. S2：version 4 / MOC 4.2

**前置：**S1 安全结构检查；S0 的真实 4.2 样本可在实施期间继续准备。

**任务：**

1. 对 `PSM__SECTIONS_V42` 每个字段建立“源表 → Document 字段 → evaluator → 5.0 encoder”映射清单，包括参数键查询、参数类型/表归属、普通颜色、增量绑定及约束。
2. 修复普通颜色的 `ver < 5` 早退与仅 5.0 参数类型检查。逐一审计 BlendShape 读取是否误用 V50 专属字段，按能力访问而非填零容错。
3. 验证普通颜色、增量形态和父级外观继承；缺失的 V50 增量通道在升级导出时写入中性值。
4. 复用现有脚本 CRUD/检查界面，确认 4.2 导入对象与原生创建对象同样可编辑；工程保存后脱离原文件重开。
5. 开放已完整实现功能的 version 4 导入；未完成跨版本功能继续诊断，不以版本号整体拦截替代功能判断。

**修改位置：**moc3 inspector/decoder/schema/encoder；core keyforms/evaluation；import_tests；现有 project 和 Editor 验收场景。

**必需测试：**乘色/屏色各自缺省与非缺省；无 BlendShape 的 4.2；Warp/Mesh 目标及共享约束；基准键端点/中间；普通关键形态与增量共存；强制访问不存在的 V50 字段应失败于检查而非越界。

**产物与门禁：**字段映射报告、真实 4.2→工程→5.0 双 Core/GPU 报告。构造测试通过但真实资产缺席时，代码可交付，S2 真实兼容仍未验收。

## 5. S3：循环参数与工程格式 v4

**前置：**S2 的参数类型/键表映射；当前工程 v1–v3 reader。

**任务：**

1. 增加 repeat 定义、输入/实际值区分，统一普通键、增量键和约束使用的实际参数值。
2. 依据双 Core 对照固定 wrap 端点、跨周期与接缝规则；非法范围/非有限值直接拒绝。
3. 读写 `param_src.repeat`；编辑桥接与面板展示循环模式并允许跨周期预览，脚本可切换和查询。
4. 引入工程 v4，集中声明本轮新增集合/字段及兼容规则；v1–v3 迁移补默认值，尚未实现的非空集合明确拒绝。覆盖快照、事务、撤销和能力诊断。
5. 更新统一验收规则中的循环例外，不改变普通参数钳制规则。

**修改位置：**core types/document/evaluation/keyforms；project codec；moc3 decoder/encoder/inspector；Godot bridge/conversions；Editor 参数面板和 API 文档。

**必需测试：**`min/max`、两侧 epsilon、正负多个周期、固定参数重复帧、接缝 A→B→A、循环约束驱动、循环与 BlendShape 组合；v1/v2/v3→v4；旧 reader 拒绝 v4；失败不更改工程。

**门禁：**定义可编辑、wrap 数值匹配、工程迁移和 5.0 再导出均通过，报告包含输入及实际值。移除循环拒绝项只能发生在这些路径齐备之后。

## 6. S4：BlendShape Glue 与 version 5 补齐

**前置：**S3 工程 v4 与完整参数求值。

**任务：**

1. 新增 Glue typed target / intensity delta，复用键表、约束、绑定和稳定 ID。
2. 实现普通强度后叠加、按规定钳制，再按既有顺序应用几何 Glue；保持每帧从基础值重算。
3. decoder/encoder 完成 `bs_glue_src` 和强度 keyform 窗口，处理基准槽及多绑定。
4. Glue 增量纳入删除引用、拓扑联合编辑、事务/快照、工程 v4、脚本 CRUD 和检查界面。
5. 补 version 5 Part/Rotation 增量、独立颜色偏移、缺省颜色的组合回归，不把 Mao 通过当成全部组合通过。

**修改位置：**core types/document/evaluation；moc3 decoder/encoder；project codec；Godot bridge；M3B/M3C 数值与编辑测试。

**必需测试：**普通 Glue 与增量共存、多绑定/共享约束、负/大于一增量及上下界钳制、非对称权重、有序多 Glue、无面端点、顶点删除引用拒绝与联合映射成功、A→B→A。

**门禁：**真实及构造 Glue 增量用例的编辑/保存/5.0 往返通过；原始 Mao 的既有严格验收无回归。新增测试明确区分普通强度与增量后钳制规则。

## 7. S5：version 6 数据、求值和 5.3 导出

**前置：**S4；S0 已证实官方 Core 5.3 数值接口可用。

**任务：**

1. 为每个 V53 section 建映射：Part Offscreen 索引、Mesh 混合、owner/flag/masks、Part key 索引、Offscreen 普通/增量 keyform 和颜色池。
2. 实现 Offscreen Document 对象、owner/键表关系、引用不变量、混合类型，升级工程 v4 读写、脚本和检查入口。禁止把无法解释的字段放入旁路 blob 冒充可编辑。
3. 对照 Purism 与官方 Core 实现普通/增量 Offscreen 求值、启用和排序，新增完整帧结构；无离屏旧模型走兼容路径。
4. 实现 5.3 schema/layout/encoder 的 480 offsets；加入 Auto/强制 5.0/强制 5.3 预检，输出模型包仍从 Document 构建。
5. Ren 24 个 Offscreen 逐对象对齐，验证 Part key 索引及 owner；补嵌套、负哨兵、空组、增量颜色和扩展 Mesh 混合构造用例。
6. 记录离屏规模、分辨率和帧资源基线，供 S6 固定缓存与预算。

**修改位置：**core types/document/evaluation/draw_order；moc3 全编解码链；project codec/package；Godot bridge/frame validation；tools/probes。

**必需测试：**全部新引用非法值/环/越界；修改 owner 与 keyform 后保存重开；删除被引用 Part；5.3 输出双 Core 加载；强制降为 5.0 无损不可达时拒绝且旧产物保留；无 5.3 特性 Auto 仍导出 5.0。

**门禁：**Ren 与最小用例的结构、编辑、持久化、原/新文件双 Core 数值一致。S5 可交付内部数值链路，但 renderer 未齐时正常 Editor 导入仍报告预览能力缺失；不提供静默扁平化画面，不标完整 version 6 支持。

## 8. S6：Offscreen 与扩展混合渲染

**前置：**S5 帧契约、数值结果和官方 Framework GPU 参考路径。

**任务：**

1. 将逻辑对象顺序编制为父子离屏渲染计划，支持开始/结束表面及正确提交父层；保持 core 不依赖 Godot 资源。
2. 实现离屏透明清屏、父子合成、Mesh/Offscreen mask、反向遮罩、动态依赖和 draw order 更新。
3. 列全固定 SDK 的 Color/Alpha 模式及合法组合，逐项实现公式、预乘 Alpha 与目标读取。通过专用小场景固定颜色空间和采样条件。
4. 实现资源池、相机/尺寸变化、禁用/空组清理、同帧读写分离及失败前置校验；测量稳定帧资源复用与峰值预算。
5. Ren 在正式 Editor 中参数预览、编辑、保存重开和再导出；原文件由官方 Framework 渲染，新文件也由官方路径复验。

**修改位置：**kasane-godot document_preview、frame validation、纹理/渲染资源与 shader；必要的新渲染计划模块；GPU 工具和 Editor 验收场景。

**必需测试：**每种受支持模式、半透明重叠、透明/不透明背景、嵌套 Offscreen、无面 mask、普通/反向 mask、目标颜色读取、屏幕尺寸/相机变化、禁用后重启、重复稳定帧与 A→B→A、对象重父级。

**门禁：**官方 Framework 与 Editor 的全图和局部图像达到统一阈值；旧三种混合与 M4/M3B 场景无回归；资源报告证明稳定帧无持续分配，超限有明确失败。独立参考路径缺失时不得通过本阶段。

## 9. S7：全版本闭环与发布

**前置：**S1–S6 对应能力门禁与外部样本补齐。

**任务：**

1. 使用同一 manifest 运行版本 2–6 与全部新增 feature 的采样矩阵，原始模型与构造模型分别报告。
2. 对每种新增可编辑语义实际修改，再保存、移走源包、重开、撤销/重做、再次导出；不能只用未编辑 roundtrip 验收。
3. 运行正式独立打包 Editor 的 PNG 建模路径与外部 MOC3 路径，记录应用携带 Core、原生库和 renderer 版本。
4. 覆盖截断/未知版本、非法引用/环、缺纹理、非有限数、文件写入失败、不支持导出目标，确认当前工程和已有输出不被部分覆盖。
5. 更新 ROADMAP、API 文档、统一验收和 M3C 完成记录；支持矩阵分列格式识别、功能完成、构造测试和真实模型/GPU 证据。

**门禁：**全部必需项目 `passed`，没有以 skip 代替的真实样本/GPU 验收；source→Document→project→export 的证据可复现。若缺资产，发布说明仅列已验收子集，M3C 总体状态保持未完成。

## 10. 验收入口与报告契约

以下命令在当前仓库已有，可用于相应阶段的回归；本次文档工作没有执行这些代码测试：

```sh
cargo check --workspace --locked
cargo test -p kasane-core --locked
cargo test -p kasane-moc3 --locked
cargo test -p kasane-project --locked
python3 tools/validate_m3b.py --help
python3 tools/validate_gpu.py --help
```

新增入口 **尚不存在，S0 起实施**。建议 CLI 为：

```text
python3 tools/validate_m3c.py --manifest <local-manifest.json> --stage S1 --output <report-dir>
python3 tools/validate_m3c.py --manifest <local-manifest.json> --stage all --output <report-dir>
```

每个阶段报告至少包括：

- 仓库/子模块 revision、工作区差异标识、构建配置、平台/Godot/Core ABI、格式版本。
- 样本路径/hash、来源类别、纹理/入口 hash、实际使用的功能、缺失依赖。
- 采样输入和实际参数值、每项比较的 expected/actual、最大误差和首个失败对象 ID/字段。
- 原文件、Document 快照、工程、再导出文件、图像及差异图路径；编辑步骤与源包脱离证据。
- 每项 `passed/failed/not_run`、失败原因、对应阶段与尚未通过的门禁；必需失败/未运行时总进程非零退出。

不得只有一个全局 success，掩盖某版本、某颜色通道或 Offscreen 未比较。数值及 GPU 阈值直接继承统一验收，不另设宽松“兼容模式”。

## 11. 实施完成记录模板

每阶段结束后在本节追加记录并链接真实产物，未运行不填写推测结果：

| 阶段 | 状态 | revision / 改动 | 用例及报告 | 未完成项 |
|---|---|---|---|---|
| S0 | passed | tools/probes/project_runtime_probe.cpp, tools/validate_m3c.py | [s0_baseline_report.json](../target/kasane/m3c/s0_baseline_report.json), [baseline_manifest.json](../target/kasane/m3c/baseline_manifest.json) | 真实 4.2、循环参数及 BlendShape Glue 外部资产待后续接入 |
| S1 | passed | modules/kasane-core, modules/kasane-godot, modules/kasane-moc3, tools/validate_m3c.py | [s1_report.json](../target/kasane/m3c/s1_report.json) | 无；Hiyori 4 个无面网格及 Glue 引用完整保留，Rice/Hiyori/Mark 导入与求值通过，v4/v6 安全门禁已建立 |
| S2 | passed | modules/kasane-moc3, modules/kasane-project, tools/create_v42_external_fixture.py, tools/validate_m3c.py | [s2_report.json](../target/kasane/m3c/s2_report.json) | 代码及构造用例交付完成；真实外部 4.2 样本验收保持 not_run |
| S3 | not_run | — | — | 循环与工程 v4 |
| S4 | not_run | — | — | Glue 增量 |
| S5 | not_run | — | — | 5.3 数值及导出 |
| S6 | not_run | — | — | 5.3 GPU |
| S7 | not_run | — | — | 全矩阵与独立应用 |
