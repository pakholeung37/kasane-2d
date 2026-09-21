# M3B：Mao 完整可编辑导入

状态：2026-09-20 制定实施设计；2026-09-21 审查修复与剩余实施已完成，完整验收通过。

2026-09-21 更新：Stage A–F 审查后的剩余契约已补齐，3,505 组双 Core 数值、打包 Editor、纹理脱离与 GPU 验收全部通过，见 [最终实施与验收记录](M3B-COMPLETION.md)。工程实际升级为 v3，以保存专用 Glue binding 和双精度旋转编辑原点；本页保留原设计要求。历史问题见 [M3B 审查记录](M3B-REVIEW.md)。

第一验收模型由用户确定为仓库本地的原始 `mao_pro.moc3`。本阶段扩展 M3 的语义覆盖，依赖既有 M1–M5；不以 M6 Viewer 为前置条件。返回 [路线图](../ROADMAP.md)。

## 1. 交付目标与边界

在正式 Editor 中导入 Mao 的 `model3.json` 和纹理，完整重建 MOC3 中的普通绑定、BlendShape、约束、Glue 与对象关系。参数预览与原模型一致；能够编辑新增语义，保存工程、移走源包、重开并再次导出有效的 5.0 MOC3。

本阶段的“完整”指上述模型内部语义，不指恢复 Cubism `.cmo3` 原工程，也不指导入动作、表情、物理、Pose 的创作数据。文件附带但未导入的信息必须在报告中明确列出。

不得通过关闭 inspector 拒绝分支、丢弃 Glue/BlendShape、烘焙默认姿势、穷举组合改写成普通 Keyform，或后台保留原 MOC3 作为预览/导出的真实来源来完成验收。

本轮仍接受 3.3、5.0 小端 MOC3，导出固定 5.0。循环参数、3.0/4.0/4.2/5.3 版本扩展、Offscreen 和新混合模式另立后续范围。BlendShape Glue 在 Mao 中为零，本轮明确拒绝，不能随普通 Glue 一并声称支持。

## 2. 调研基线与实测清单

源码基线：主仓库 `a452e1221eb2b353537b0cb15d622d18312a7d0c`；PurismCore 子模块 `9a2285ad731089c368eaad4cbe3a6b1f8a588f19`。gd-cubism 源码在主仓库中。下列数字读取原始二进制的 section/count 表，按 PurismCore `moc3.h` 字段顺序解释；这是结构盘点，不是数值兼容验收。

- 模型入口：`demos/gd-cubism-demo/assets/live2d/mao/runtime/mao_pro.model3.json`。
- MOC3 SHA-256：`247d028f9900be2a46e4530816ece5749524d35013ba77424bd943598e0e54ff`。
- 文件版本值 `5`；画布 `5800 × 8400`；PPU `5800`；文件原点 `(2900, 4200)`；`canvas.flag = 0`。
- 纹理：槽 0，`mao_pro.4096/texture_00.png`，全部 Mesh 使用此槽。验收入口须另记录实际图片和 model3 的 SHA。

| 项目 | Mao 实测 | 实施含义 |
|---|---:|---|
| Part | 31 | 普通组织与绘制分组继续保留 |
| Deformer | 175：Warp 116、Rotation 59 | 嵌套变形必须与增量叠加联动 |
| ArtMesh | 260；6012 个顶点 | 真实整模预览与编辑测试 |
| 参数 | 128：Normal 95、BlendShape 33 | 新增参数类型，不能全部读写为 Normal |
| 普通绑定 / key table | 138 / 127 | 保留现有普通插值语义 |
| BlendShape key table / binding | 33 / 124 | 独立基准键、增量形态与约束引用 |
| BlendShape 目标记录 | Warp 3、Mesh 31、Part 1、Rotation 3 | 四类都属于 P0，不仅实现顶点位移 |
| 基准键索引 | 17 个为 0，16 个为 1 | 覆盖基准位于端点和中间两种情况 |
| BlendShape 约束 | 7 条曲线、14 个值、234 个约束索引引用 | 保留共享引用和多约束组合 |
| Glue | 7；322 条 info，即 161 对顶点 | 保留两个端点各自的权重与连接顺序 |
| Glue Keyform | 7，强度均为 1 | Mao 不足以验证参数驱动强度，补构造用例 |
| 普通绘制数据 | 2 个 draw group、65 条 mask 引用 | 覆盖已有排序与遮罩逻辑 |
| drawable flags | 227×4、15×5、10×12、8×6 | 含普通/加法/乘法混合与反向遮罩 |
| 循环参数 / Offscreen / BlendShape Glue | 0 / 0 / 0 | 不阻塞 Mao；仍明确拒绝这些范围外语义 |

约束参数为 `ParamA`、`ParamI`、`ParamU`、`ParamE`、`ParamO`、`ParamMouthDown`、`ParamMouthAngry`。这些参数本身也是 BlendShape 参数；不能假定约束只引用 Normal 参数。

示例驱动还包括眉毛、头发、兔子的位置/大小/旋转/绘制顺序、光效与整体颜色。验收应按实际目标及通道自动生成清单，不从参数名字推断覆盖范围。

现有 `external_v50` fixture 仅有 1 Mesh、2 Deformer、2 参数，没有 Glue/BlendShape。现有 `test_unsupported_features_rejected` 将 Mao 作为拒绝用例。原 M3 的“已验收”只适用于旧范围，不代表本阶段已完成。

## 3. 已核对的实现及关键语义

### 3.1 PurismCore

| 源码 | 已核对内容 | 对实施的约束 |
|---|---|---|
| [moc3.h](../../modules/purism-core/src/moc3.h) | V30 Glue、V42 BlendShape、V50 扩展通道的源表 | 按字段映射，不序列化运行时指针 |
| [model.c](../../modules/purism-core/src/model.c) | 参数类型、表归属、约束指针、目标与绑定窗口的连接 | Document 使用稳定 ID；稠密索引只存在于读写/求值适配层 |
| [param.c](../../modules/purism-core/src/param.c) | BlendShape 键查找、base key 排除、约束曲线 | 独立于普通多轴笛卡尔积绑定 |
| [blendshape.c](../../modules/purism-core/src/blendshape.c) | 增量叠加、通道钳制、颜色缺省 | 各类型差异必须显式表达 |
| [glue.c](../../modules/purism-core/src/glue.c) 与 [interpolate.c](../../modules/purism-core/src/interpolate.c) | 强度求值、顺序连接 | 两端基于同一次读出的坐标更新 |
| [update.c](../../modules/purism-core/src/update.c) | 完整求值顺序 | Glue 位于变形之后、Y 翻转之前 |
| [verify.c](../../modules/purism-core/src/verify.c) | Glue info 成对、顶点范围、BlendShape 窗口与颜色偏移检查 | 校验覆盖解码、编辑、导出三条入口 |

**BlendShape 绑定与约束：**

1. `param_src.type` 区分普通参数和 BlendShape 参数。一个参数可以拥有多张 BlendShape key table，不按 Mao 恰好 33 参数/33 表简化。
2. key table 保留关键值和 `base_key_idx`；基准键不产生增量。跨基准键插值时，只计入非基准侧；普通段最多使用两个增量形态。不能把默认参数值等同于基准键。
3. 键区间外的选择遵循 `psm__resolve_blend_key_tables` 的端点行为；不能套用普通 key table 的越界禁用规则和 epsilon。
4. constraint 按其引用参数的实际值，对 `(key, weight)` 曲线线性插值，区间外取端值。一个绑定的总约束权重从 1 开始，对各约束取 `min`，不是相乘。
5. 普通形态求值后，将每个绑定的增量乘以插值权重、约束权重并相加；保持目标记录及绑定的源顺序。基准槽的文件窗口也要完整校验。
6. 每帧从普通求值结果开始，不能把上帧已叠加数据再次累加。必须测试参数不变、仅约束参数改变及 A→B→A。

**5.0 BlendShape 通道：**

| 目标 | 增量通道 | PurismCore 本轮基线行为 |
|---|---|---|
| Mesh | 顶点、opacity、draw order、multiply/screen RGB | opacity/RGB 叠加后限制到 `[0,1]`；顺序特殊处理见下 |
| Warp | 控制点、opacity、multiply/screen RGB | 先叠加，再进入父变形与外观继承 |
| Rotation | origin x/y、angle、scale、opacity、multiply/screen RGB | angle 限制到 `[-3600,3600]`，scale 到 `[0.0001,100]`；反射不作为增量通道 |
| Part | draw order | 不是 Part opacity 动画；保留已有输入透明度语义 |

draw order 的普通结果已转为整数；增量叠加后加 `0.001`、限制到 `[0,1000]`、再转整数。不能把所有浮点值最后统一四舍五入。颜色读取使用每个增量 Keyform 的 `key_mul_color_off` / `key_scr_color_off`，与普通对象的 `key_color_off + index` 分开。负颜色偏移表示没有 override；当前 Core 在双键插值任一端缺颜色时跳过该次颜色贡献，须用官方 Core 对照确认并锁定行为，不能自行补零。

上述规则是所固定 PurismCore 源码的行为；官方 Core 数值对照是最终兼容门禁。发现分歧时先定位并修正基线或适配，不以两套自有实现互相一致作为通过证据。

**Glue：**

源字段包括 ID、普通 binding、intensity Keyform、两端 Mesh、info 窗口、每端顶点索引与 weight。info 按相邻两条配对。

对当前坐标 `a`、`b`，先求 `d = b - a`，然后写入 `a + d * intensity * w0` 和 `b - d * intensity * w1`。不能假设两端 weight 相等或归一化。多个 Glue 及同一 Glue 内的点对顺序影响结果，Document 和工程必须保存显式顺序。普通强度插值不照搬 BlendShape Glue 的 `[0,1]` 钳制。

Glue 的 `local_enable` 字段存在于内部结构，但当前 `psm__apply_glues` 没有按它分支；也没有按 drawable 可见性过滤。不能仅凭字段名新增启用语义。隐藏/禁用端点及可能的缓存行为作为专项数值用例，验证后再决定 Document 如何表达必要状态。

### 3.2 gd-cubism

[internal_cubism_user_model.cpp](../../modules/gd-cubism/src/private/internal_cubism_user_model.cpp) 负责读取 model3、加载 MOC3、纹理、Expressions、Physics、Pose、UserData、Motions 及 EyeBlink/LipSync 分组。`pro_update` / `efx_update` / `epi_update` 在 Core `Update()` 前可修改参数。它使用 Core 完成 BlendShape/Glue，不是第二套可提取的编辑算法。

[internal_cubism_renderer_2d.cpp](../../modules/gd-cubism/src/private/internal_cubism_renderer_2d.cpp) 消费最终 drawable 顶点、UV、render order、opacity、颜色和 mask。坐标上传乘 PPU 并反转 Y，UV 使用 `1-v`；资源复用包括位置纹理、参数纹理、共享遮罩几何、mask atlas、相邻同 shader/纹理批次。mask atlas 采样变换有上一帧配对处理。

[internal_cubism_renderer_resource.cpp](../../modules/gd-cubism/src/private/internal_cubism_renderer_resource.cpp) 为普通/反向遮罩与三种混合选择材质。新 BlendShape/Glue 最终仍输出同一 DrawableFrame，原则上不需要新增专用 shader；变化后的 bounds、排序、颜色和遮罩必须触发正确刷新。

对照时关闭 motion、expression、physics、pose、effect 等自动驱动，逐项记录配置并确认实际参数值一致，固定 Part 输入透明度和相机。只关闭 motion 不足以获得静态参数基准。截图等待 mask 与主体同步稳定后获取，并验证连续参数更新。

gd-cubism 保留为独立对照，不替换 Editor 的 Rust renderer。它对扩展混合模式存在兼容映射，不能作为未来 5.3/Offscreen 完整支持的依据。

## 4. Feature 清单与数据设计

以下类型和接口名称是实施建议，尚不存在。先稳定语义和不变量，再定具体 Rust/GDScript 签名。

| ID | 必须实现 | 主要改动位置 | 完成条件 |
|---|---|---|---|
| F01 | 安全结构检查与兼容性报告分离 | `kasane-moc3/inspector.rs`、Godot 文件桥接 | 安全解析后一次列出全部阻塞项、数量、对象/字段；损坏文件不继续解引用 |
| F02 | Parameter 类型 | core types/document、codec、bridge、参数面板 | Normal / BlendShape 导入、编辑、保存、导出一致；非法绑定类型组合拒绝 |
| F03 | BlendShape key table、增量 binding、constraint | core、新增读写映射 | 稳定 ID、基准键、共享约束、目标类型和顺序均可检查与编辑 |
| F04 | Mesh/Warp 增量求值 | core evaluation/keyforms | 位置、颜色、opacity、顺序按目标类型正确叠加 |
| F05 | Part/Rotation 增量求值 | core evaluation/draw_order | Part 排序及 Rotation 所有支持通道与继承关系正确 |
| F06 | Glue 对象、普通强度绑定与求值 | core、moc3 decoder/encoder | 161 对连接完整恢复；强度可编辑；有序应用且坐标正确 |
| F07 | 引用、拓扑、事务与撤销 | core document、Godot snapshots | 新语义参与删除检查、批量拓扑替换、回滚和 Undo/Redo |
| F08 | 工程格式升级 | project codec/store | 新模型脱离原包可重开；旧程序不能静默丢字段 |
| F09 | 完整 MOC3 5.0 再导出 | schema/layout/encoder、package | 从 Document 构建非空扩展表，两个 Core 都接受且结果一致 |
| F10 | Editor 检查与脚本编辑 | bridge、project_io、API.md、各 dock | 区分普通/增量绑定，定位约束和 Glue 两端，完整返回诊断 |
| F11 | 原始 Mao 集成验收 | tools、测试和现有探针 | 原文件→Document→工程重开→新文件的结构/数值/GPU证据齐全 |

### 4.1 Document 建议

- `ParameterKind`：Normal / BlendShape；既有工程参数默认 Normal。
- `BlendShapeKeyTable`：ID、parameter ID、严格递增 keys、base key index。约束曲线用独立稳定 ID，可被多个 binding 引用。
- `BlendShapeBinding`：ID、typed target、key table ID、有序约束引用、按 key 对齐的 typed delta keyforms。允许目标同时有普通 binding 与多条增量 binding；旧的单普通 binding 规则继续有效。
- delta 使用单独的结构，缺省为零；颜色 override 用显式可选状态。不能复用 `Appearance::default()` 的乘色 1、opacity 1，也不能用绝对形态的非负检查拒绝合法负增量。
- `Glue`：内部 ID、runtime ID、名称、mesh A/B ID、有序的 `(vertex A ID, vertex B ID, weight A, weight B)`；基础强度和普通多轴强度 binding。稳定 vertex ID 在导出时转换为 u16 索引。
- 对象类型、参数类型、关键值、引用、有限数、数组长度与可写出容量全部进入统一校验。Mao 所没有的合法边界通过小型用例验证；未支持语义返回具体错误。

根对象绝对位置目前通过 `to_parent_positions` 引入画布原点/PPU；增量是向量，不能走同一套点转换。建议根级 delta 以编辑器像素向量保存，转换只应用缩放和方向，不加原点；有父变形器时使用现有父局部单位。Rotation origin delta 同理。读写、脚本 API 和选择覆盖层须书面定义单位，并验证 `flag=0/1`、非零原点、不同 PPU、父关系变更。

### 4.2 求值结构

当前 `evaluation.rs` 在逐 Mesh 循环中完成父变形和 Y 翻转，需要引入全部 Mesh 共用的 Glue 阶段。逻辑顺序为：

```text
参数实际值 → 普通/BlendShape 键选择与约束权重
→ 普通 Part/Deformer/Mesh/Glue 形态
→ 各对象 BlendShape 增量与通道钳制
→ 嵌套变形、外观继承、Part 透明度
→ 已变形 Mesh 之间按顺序应用 Glue
→ 最终 Y 方向转换、draw group 排序、DrawableFrame
```

可维持拓扑遍历，在每个对象进入父变形之前叠加其增量；但所有 Glue 必须等两端网格的变形完成。只在渲染层修顶点会导致数据观察、导出对照与编辑器画面不一致。

算法实现遵循当前 Rust core 架构，优先抽取/复用 PurismCore 的纯数据算法；当前公开 `PurismKeyform.h` / `PurismDeformer.h` 没有覆盖整个 BlendShape/Glue 流程。若沿用既有 Rust 移植方式，必须记录来自上述 C 函数的逐项映射、保留许可说明，以双 Core 数值测试证明等价；不得新增近似插值，也不得为了调用算法每帧编码并 revive MOC3。具体共享纯函数或 Rust 移植方案在阶段 B 固定。

### 4.3 引用和拓扑编辑

删除 Parameter 前检查普通轴、BlendShape 表、constraint；删除 Mesh 前检查 mask、Glue、BlendShape target；删除约束/表前检查全部 binding。错误报告列出引用者。

Mesh 改拓扑必须在一次候选提交内更新普通 Keyform、全部增量顶点数组、Glue 顶点映射和索引。缺任一映射则拒绝，不能静默丢连接。Warp 网格行列变化同样更新所有增量控制点；更换父变形器需要显式坐标迁移或拒绝不完整请求。Document snapshot、transaction、ChangeSet、Undo/Redo 必须覆盖新增集合和顺序。

### 4.4 MOC3 与工程文件

读写映射至少包括：

| 数据 | 文件字段 |
|---|---|
| 参数类型及表归属 | `param_src.type / blend_key_table_off / blend_key_table_len` |
| 增量键表 | `blend_key_table_src.keys_off / keys_len / base_key_idx` |
| 增量绑定 | `blend_binding_src.key_table_idx / key_bs_off / key_bs_len / bs_constraint_idx_off / bs_constraint_idx_len` |
| 目标表 | `bs_warp_src / bs_art_mesh_src / bs_part_src / bs_rotation_src` |
| 约束 | `blend_constraint_idx_src / blend_constraint_src / blend_constraint_val_src` |
| 增量形态 | 各类型 keyform 池、`key_pos_src`、逐 Keyform 颜色偏移及颜色池 |
| Glue | `glue_src / glue_info_src / glue_key_src`；普通强度绑定进入普通 binding 表 |
| 参数关键值查询 | `param_keys_src` 中普通/增量参数的对应关键值集合 |

`schema.rs` 已有扩展字段名称不等于 encoder 支持；当前 encoder 将参数类型和 BlendShape 表长度写零。实施须重新分配普通/增量形态窗口，保留基准槽位置及缺颜色标记，逐项检查 offset/count/index/ID/u16 容量，不复制原始文件窗口绕过编辑数据。

工程建议升级到 `format_version = 2`：新程序读取 v1 并补空扩展集合、Normal 类型，保存统一写 v2；旧程序已有版本拒绝逻辑，防止忽略未知字段后把模型另存为损坏状态。版本迁移需覆盖普通项目与含扩展项目；禁止只给 v1 添加可被旧 reader 忽略的字段。项目纹理继续内置，源路径与 SHA 仅作来源信息。

### 4.5 Editor 和附件

P0 保持已有 model3 入口，补齐完整导入摘要、阻塞对象、纹理状态，以及新类型的脚本 CRUD/批量编辑、快照和检查面板。参数面板区分 Normal/BlendShape；对象检查可以查看增量键与约束、跳转 Glue 两端。预览值、普通 Keyform 编辑和增量编辑为三种明确操作。

P1 再提供裸 MOC3 文件选择与纹理槽映射 UI、纹理重定位界面、DisplayInfo 名称/分组辅助导入。这些不是 Mao 首次可编辑导入的核心阻塞，底层裸 MOC3 API 已存在。

Mao 附带 Physics、Pose、DisplayInfo、8 个 Expression 和 7 个 Motion；model3 还包含 Groups、HitAreas。当前 importer 仅枚举 FileReferences 中的未导入附件，顶层 metadata 也应纳入报告，防止用户以为已保留。第一阶段不要求播放这些附件，不承诺原样再导出运行包行为；附件保留/重映射与创作功能另行设计。

## 5. 实施顺序与阶段门禁

| 阶段 | 工作 | 进入下一阶段的条件 |
|---|---|---|
| A：原始模型基线 | 实现安全 inventory/特性报告；固定模型、纹理、Core、参数集合；建立原始 Mao 双 Core 数据与 gd-cubism 图像基线 | 原文件一致性检查及两 Core 对照结果可复现；分歧有定位，未解决阻塞不能算通过 |
| B：数据契约 | F02/F03/F06 的类型与校验、稳定引用、坐标和有序集合；固定算法复用方式；v2 迁移 | 可构造/保存/读取新结构，负例与原子性通过 |
| C：BlendShape | F04/F05，读写四类目标、约束和颜色；同步脚本接口 | 小模型双 Core 对照，端点/中点/基准键/多约束与保存往返通过 |
| D：Glue 与整模导入 | 强度绑定、顶点映射、求值阶段调整；候选 Document 整体验证后提交 | 原始 Mao 无丢弃导入，结构清单与数值采样通过 |
| E：编辑和应用闭环 | F07/F09/F10；逐类编辑、Undo/Redo、移走源包重开、正式 Editor 包 | 编辑确实改变新 MOC3，新增语义可在应用中检查与操作 |
| F：完整验收 | F11、GPU 局部/整图、性能测量与既有回归 | 所有必需项 passed；缺环境/输入明确 not_run，不能宣布完成 |

阶段 C/D 的顺序用于控制改动规模，不意味着 BlendShape 完成后可以提前让 Mao 以丢 Glue 的方式成功导入。只有全部实际特性可表示时才开放该模型的正式导入。

## 6. 验收计划

沿用 [统一验收规则](../VALIDATION.md)，新增 `tools/validate_m3b.py` 作为计划中的入口，报告目录拟为 `target/kasane/m3b/`。**该入口当前不存在**，实现时再公布可执行命令。

### 6.1 结构与数值

1. 对原始 Mao、导入 Document、保存重开 Document、重新导出文件在 PurismCore/官方 Core 的结果按 runtime ID 对齐。保存全部采样输入与失败对象。
2. 源表到 Document 的映射覆盖四类目标、124 个增量绑定、7 条约束/234 个引用、7 个 Glue/161 对连接，不能只比较最终 drawable 数量。表共享允许规范化，但引用语义、形态和执行顺序必须证明保留。
3. 128 参数全部默认值、范围端点、普通/增量关键值及中点；另覆盖 constraint 曲线键与中点。组合采样按“同一目标多个增量驱动”“驱动×约束”“普通变形×增量×Glue”依赖生成；记录清单，不穷举 128 维。
4. 专项：base key 为首/中/末、仅一个有效非基准形态、越界参数实际值、负 delta、多个约束取最小值、仅约束变化、颜色缺省、颜色/opacity/angle/scale 钳制、绘制顺序取整、A→B→A 和重复求值。
5. Glue 补充：非 1 强度、普通多轴强度绑定、不对称权重、连续共享顶点、多 Glue 顺序、同 Mesh 两端、隐藏/禁用端点、不同父变形器、canvas flag 两种方向。Mao 的强度全部为 1，不能代替这些测试。
6. 增量坐标覆盖根级、非零原点、PPU、嵌套 Rotation/Warp。源模型 flag=0 不得被默认 flag=1 覆盖。
7. Float 阈值 `1e-5 + 1e-5 * max(abs(actual), abs(expected))`，位置还须不超过 `0.05` 原画像素；离散字段精确一致。禁用对象的运行缓存若有状态依赖，显式记录采样历史、区分结构与可观察帧，不把不可见当作任意丢数据的理由。

### 6.2 编辑、持久化与失败

- 分别修改 Mesh/Warp delta、Rotation origin/angle/scale、Part draw order、增量颜色、base key、constraint 曲线、Glue 强度与连接权重，再导出验证；覆盖新增、重绑定、删除已解绑对象。
- 含普通和增量形态的 Mesh 改拓扑并更新 Glue 映射；不完整映射失败且原文档不变。修改前后、Undo/Redo、保存重开均进行对照。
- 保存后移走原始 MOC3、model3 和纹理目录，独立重开、编辑、导出；不得读取隐藏的源运行模型缓存。
- 非法 target/type、base key 越界、非递增 keys、未知参数类型、constraint 悬空/非有限值、增量数组长度错误、Glue 奇数 info/越界顶点、坏图片、文件截断、未知版本和范围外语义均给出具体诊断。
- 导入失败不替换当前文档；保存/多文件导出失败保留原产物。更新旧的 Mao 拒绝测试为成功闭环用例，并保留独立构造的范围外拒绝测试。Mao 缺失应使必需验收失败，不能沿用 `if exists` 跳过。

### 6.3 GPU、应用与性能

- 原始模型使用独立 gd-cubism，编辑模型使用现有 Rust renderer；至少包含官方 Core 参考。统一参数、Part 输入透明度、相机、渲染后端、分辨率、纹理过滤与背景。
- 检查默认姿势及嘴形约束、眉毛、兔子变换/排序、光效/颜色、Glue 接缝的非默认姿势；局部区域按实际对象选取，保存对象 ID、截图、差异图。
- 连续变化参数和相机后检查遮罩同步、bounds、排序与观察 ready 状态；数据几何已通过仍需 GPU 检查。
- 采用统一规则的整图 MAE `<=0.005`、通道误差 `>0.05` 像素比例 `<=1%`，关键局部同阈值；构造采样区域每通道 `<=2/255`。
- 使用正式打包 Editor 完成导入、选择、参数预览、脚本编辑、保存重开、导出，不能只跑库测试或旧 demo。
- 记录导入各阶段耗时、峰值内存、首帧 ready、参数连续拖动的 p50/p95、保存/导出耗时与 GPU 资源数。当前没有 Mao Editor 性能基线，阶段 A/B 后确定可复现硬件上的响应目标；测量前不承诺帧率，不预先为性能扩大架构。

现有回归入口：`cargo test -p kasane-core -p kasane-moc3 -p kasane-project --locked`、`cargo test -p kasane-godot --lib --locked`、`cargo check --workspace --locked`，以及既有 M3/Godot/GPU 验证工具。新报告记录每条实际运行命令与状态；本设计文档不把历史结果或未执行命令记为通过。

## 7. 当前结论与后续范围

Mao 的确定阻塞是 BlendShape 四类目标及约束、Glue 和这些语义的完整编辑/保存/导出链路。既有 renderer 的输入形态可以继续使用，主要架构调整在 Document 求值阶段与引用管理。

仍需阶段 A/B 用官方 Core 明确的边界包括颜色缺省插值、禁用端点参与 Glue 的状态行为、特殊键表/基准窗口及极端数值；这些是待验证项，不是推定支持。DisplayInfo 辅助元数据、裸 MOC3 导入 UI、循环参数、其他文件版本、BlendShape Glue、Offscreen、动作/物理/表情分别列为后续工作，不混入 Mao 完成标准。
