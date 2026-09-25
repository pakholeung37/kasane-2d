# 编辑器动画资产与 Live2D 运行时包规划

架构更新：当前工程格式为 v6，读取 v1–v6；原方案中的 v5 已由带 typed model3 引用的 v6 替代。具体迁移与 API 变化见实施记录。

状态：P0–P6 的 Rust/Python 编辑、v6 存储（兼容 v5 迁移）、typed wire、CPU 预览及资源包闭环已落地。官方 Framework 加载、Physics 长序列差分，以及 18 个 Motion/Pose GPU 帧（含嵌套 Offscreen 与宿主显式应用 Model opacity）已通过。Model EyeBlink/LipSync 动态映射按原计划后置并有预览覆盖诊断。具体证据与复现命令见 [实施记录](EDITOR-ANIMATION-IMPLEMENTATION.md)。日期：2026-09-25。

目标是让 Kasane 编辑器能创建、导入、编辑、保存、预览和导出 Expression、Motion、Physics、Pose、CDI；最终产物为可交给 Live2D 宿主加载的模型资源包。这里的“完整运行时包”指模型及其关联资源，不包含 SDK 二进制、播放器应用或 Cubism 编辑器工程格式。

对照基准是本地 `third_party/CubismSdkForNative-5-r.5/Framework/src` 的实际代码，样例取自同目录 SDK 的 `Samples/Resources`。以下新增类型、模块和 API 名称均为设计提案，不代表现有接口。

## 1. 范围与完成标准

本轮包含：

- Expression：参数条目、三种混合模式、淡入淡出、多表情预览、exp3 导入导出。
- Motion：时间轴曲线、关键点/切线编辑、事件、淡入淡出、循环和组合预览、motion3 导入导出。
- Physics：rig 编辑、输入输出映射、粒子与归一化设置、确定性预览、physics3 导入导出。
- Pose：部件互斥组、关联部件、切换参数、淡入、pose3 导入导出。
- CDI：显示名、参数组树、组合参数、cdi3 导入导出。
- 最小更新调度器、运行时参数状态、时间定位、Observer 动画帧捕获。
- model3 关联资源管理与整包导出，包括贴图；新建工程和导入工程都必须支持。

后置：自动 EyeBlink、Breath、Look、LipSync 驱动器、音频分析/播放、面捕、通用实时插件系统、GUI 时间轴与图形曲线控件。本轮先完成现有 Rust/Python 编辑器 SDK 的能力与可观察结果，不以恢复 GUI 为前置条件。

EyeBlink/LipSync 的 model3 Groups，以及 motion3 内已有的同名 Model 曲线仍须导入、保存、导出。其动态映射预览后置：遇到它们要报告预览覆盖不足，不能静默忽略后声称完全一致。普通 ParamEyeOpen、ParamMouthOpen 等 Parameter 曲线正常支持。

完成标准是同一工程经过“导入/新建 → 编辑 → undo/redo → 保存重开 → 定位/预览 → 导出 → 官方 Framework 加载验证”形成闭环，不能仅以文件存在或 JSON 可解析验收。

## 2. 当前代码约束与新增发现

| 位置 | 当前行为 | 对方案的影响 |
| --- | --- | --- |
| `kasane-moc3/src/importer.rs` | Moc、Textures 之外附件进入 `unimported_attachments` | 必须把整包解析提升到 project 层；保留裸 MOC3 导入入口 |
| `kasane-moc3/src/encoder/context.rs` | MOC3 编码顺便生成最小 model3 | model3 完整组装迁出二进制编码职责，过渡期保留旧 artifact 兼容字段 |
| `kasane-project/src/package.rs` | 原子发布 MOC3、model3、PNG；validator 只接收 Moc3Artifact | 增加整包计划、引用图、附件验证与整包校验入口 |
| `kasane-core/src/document.rs`、checkpoint/history | 文档状态、撤销、modified 判定有多份内容视图 | 新集合必须进入所有快照、大小估算、引用及历史路径 |
| `kasane-core/src/types.rs::Part` | 有 enabled、层级、draw order，没有运行时输入 opacity | Pose 需要独立部件透明度状态，不能借用 enabled 或 offscreen opacity |
| `kasane-core/src/evaluation/parameters.rs` | 未知参数报错；repeat 采用当前 Kasane 规则 | 新增兼容运行时状态，不改变现有 authoring evaluate 的契约 |
| `kasane-core/src/preview.rs` | 缓存键是 generation/document/preview revision | 动画时间、激活片段、物理状态和事件历史必须进入独立缓存身份 |
| `kasane-project/src/codec` | 当前写 v4、读 v1–v4 | 新版本不能让旧 reader 静默丢掉动画；计划写 v5、继续读 v1–v4 |
| `kasane-sdk-observe` | 由参数映射重新求值并捕获 frame | 新增捕获已求值动画快照的路径，不能再次采样而丢掉 Pose/Physics 状态 |

特别需要先用 probe 锁定的行为：

1. `Motion/CubismMotion.cpp` 的 PartOpacity 分支调用 `GetParameterIndex`、`SetParameterValue`，不是 `SetPartOpacity`。应保留轨道类别并写同名真实/虚拟参数；Pose 再消费它。
2. `Effect/CubismPose.cpp` 以 PartId 同时查 parameter 和 part。不存在同名 MOC3 参数在 Framework 中可成为虚拟参数，这是正常路径，不能一律当导入错误，也不能给 MOC3 强行加参数。
3. Framework 的参数 clamp/repeat、权重应用顺序与 Kasane 现有求值不一定一致；特别是 repeat 区间最大值与跨边界插值。新预览状态必须有明确 Cubism 兼容策略，并验证进入几何求值时不会二次归一化改变结果。
4. Motion 默认 `MotionBehavior_V2`；Bezier 有 restricted/unrestricted 路径；循环边界有专门处理。不能只做线性时间轴就称支持 motion3。
5. Physics 的 Fps 缺失或为 0 时使用传入 delta，Fps > 0 时固定子步并插值。不能无条件改成一个固定频率并声称匹配原文件。
6. CDI 包含 `Parameters`、`ParameterGroups`（GroupId 父链）、`Parts`、`CombinedParameters`；Physics Meta 中还有用于编辑的 `PhysicsDictionary`。

## 3. 模块边界

建议增加两个有独立职责的 crate，不把所有功能塞入 `kasane-moc3`：

| 模块 | 职责 |
| --- | --- |
| `kasane-core` | 动画资产领域类型、文档集合、结构不变量、引用关系；几何求值新增运行时输入通道 |
| 新 `kasane-live2d` | model3/exp3/motion3/physics3/pose3/cdi3 的 typed wire、解码/编码、格式诊断；不执行文件 IO、不持有播放状态 |
| 新 `kasane-animation` | 编译编辑资产、运行时参数状态、Motion/Expression/Pose/Physics 求值、调度、seek/replay；依赖 core，不依赖 project 或 GPU |
| `kasane-moc3` | 保持 MOC3 二进制读写、runtime ID 与 texture slot 映射 |
| `kasane-project` | 资源解析与存储、工程迁移、整包导入、ExportPlan、原子发布；调用 live2d 和 moc3 |
| `kasane-sdk` | 编辑事务、跨资产引用与历史、session 生命周期、创建动画预览会话；依赖 animation |
| `kasane-python` | typed records、API 适配和脚本工作流；不再实现另一份混合、曲线或物理算法 |
| `kasane-sdk-observe` / renderer | 渲染动画快照，保持参数与物理求值规则在 CPU 层 |

依赖保持单向：core ← live2d / animation / moc3；project → core + live2d + moc3；sdk → project + animation。没有 animation → sdk 回依赖。

## 4. 领域模型与引用

持久化资产进入 `Document` 的 `AnimationAssets` 聚合，使用稳定 UUID、确定顺序和细粒度 edit 命令。播放游标、物理缓存、激活表情和选择状态不属于文档内容，不写工程，也不进入 undo 历史。

| 资产 | 核心字段 |
| --- | --- |
| `Expression` | UUID、名称、fade in/out、有序 entries（parameter ref、value、Add/Multiply/Overwrite） |
| `MotionClip` | UUID、名称、duration、fps、loop、Bezier evaluation metadata、默认 fade、tracks、events |
| `MotionTrack` | UUID、Target 类别、target ref、可选逐曲线 fade、首点及 typed segments |
| `MotionSegment` | Linear / Bezier / Stepped / InverseStepped；稳定编辑身份、终点、Bezier 控制点 |
| `MotionEvent` | UUID、时间、字符串；同时间事件保留顺序 |
| `PhysicsAsset` | 有序 rigs、fps、gravity/wind、dictionary；导出为一个 physics3 文件 |
| `PhysicsRig` | UUID、外部 Id、显示名、normalization、inputs、outputs、particles |
| `PoseAsset` | fade、ordered groups；每组 ordered entries（part ref、linked part refs） |
| `DisplayInfo` | 参数组树、参数 group membership、combined parameter sets、显示字段来源信息 |
| `ModelSettings` | expression 注册名、motion group/entry、fade override、可选 sound asset、Groups、Layout、HitAreas、UserData 关联 |

约束与策略：

- 已解析的参数、Part、Mesh 引用使用内部 UUID；export 时统一映射到 runtime ID，名称绝不作为引用身份。
- `RuntimeParameterRef` 区分真实参数引用与虚拟 runtime ID；Pose 的同名控制参数属于合法虚拟槽。Physics 真实输入/输出需要可取得 min/max/default 的实体参数，不把所有缺失引用都虚拟化。
- 未解析的普通附件目标保留原 runtime ID 并提供诊断；严格整包发布要求修复或显式允许明确的 Framework 兼容情形，不能静默删曲线。
- runtime ID 的唯一性按参数、Part、Drawable 等命名空间校验；允许 Pose 所需的跨命名空间同名。所有格式共用导出映射，不能各自生成 ID。
- 显示名以现有 Parameter/Part.name 为单一来源；CDI 导入更新它们。分组及 CombinedParameters 独立存储，不维护第二套可冲突的显示名。
- motion group entry 与 clip 分离：同一 clip 可注册到多个组，Sound/fade override 属于注册条目。
- clip duration 包含事件和曲线时间；编辑可显式裁剪或扩展。负时间、非有限数、非法 segment、无效控制点和越界引用在事务提交前诊断。
- model3 fade override、motion 默认 fade、曲线 fade 的缺省/继承/零秒必须区分；导入保留可表达的省略信息，默认值由格式 codec/兼容配置解释。
- 参数/Part 删除、runtime ID 更改、组重排和物理粒子重排都走统一引用检查；默认拒绝悬挂引用，显式重映射必须同一事务完成。
- keyform 的参数轴与 Motion 的时间轴是不同数据，禁止复用 `MeshKeyform` 表示 motion keyframe。
- 首轮新增字段集中到一个 v5 工程版本，旧工程补空集合。旧 v4 reader 必须拒绝 v5；不允许为兼容而写 v4 并丢动画。

未知 JSON 字段在 wire 扩展区保留；遇到未知可执行语义标为未支持。对未知字段中的 ID/路径不猜测重写：如果编辑导致潜在失效，严格导出阻断并说明字段与原因。目标是已支持字段的语义往返，不承诺原 JSON 字节、空白和键序完全一致。

## 5. 编辑接口与编辑器行为

每类对象提供 list/get/create/replace/delete，以及必要的细粒度命令。Rust/Python 同步开放，沿用 expected_version、对象 handle、事务取消与结构化错误。

建议的操作族（示意）：

```text
edit.create_expression / set_expression_entry / set_expression_fades
edit.create_motion / create_motion_track / insert_motion_key / move_motion_key
edit.set_motion_segment / set_motion_event / set_motion_timing
edit.create_physics / replace_physics_rig / set_physics_input / set_physics_output
edit.replace_pose / set_pose_group / set_pose_links
edit.create_parameter_group / move_parameter_to_group / set_combined_parameters
edit.set_model_settings / register_expression / register_motion
```

时间轴首轮支持增删移动关键点、编辑 Bezier 切线/segment 类型、修改 duration/fps/loop 和事件。时间缩放、曲线镜像、物理烘焙、复杂切片拼接后置；用户可以通过基础命令组合，不将其加入首轮完成条件。

替换大 clip 或 physics rig 时避免复制全部模型的额外中间状态；复用现有 checkpoint 路径并补足内存预算测试。所有新增对象参与 `ObjectKind`、references、diagnostics、modified、history size、save/open。

CDI 分组编辑不应触发几何重编译。建议区分内容 revision、rig/evaluation revision、animation revision、resource revision；第一阶段允许保守动画失效，但必须保证正确，优化再分域。

## 6. 预览与更新调度

保持现有 `evaluate(values)` 无时间、无副作用。新增 `AnimationPreview`，绑定不可变的文档版本快照；播放不改变工程的 modified 状态。文档改变后预览显式刷新，清理或重建不再适用的状态，不能混用旧物理缓存与新参数表。

推荐执行流程：

```text
初始值/基准快照 + 编辑器输入
  → Motion（写真实/虚拟参数、Model opacity）
  → 保存 motion 基准参数
  → Expression（默认顺序 300）
  → Physics（600）
  → Pose（800，写 Part opacity）
  → Core 几何/层级/透明度求值
  → DrawableFrame → Observer / WGPU
```

Motion 前恢复上次保存的 motion 基准，避免把上一帧 Expression/Physics 结果反复累加到基准。编辑器输入是在基准阶段覆盖；需要最终强制参数的调试选项必须单独命名并记录在快照中。

最小 scheduler 用稳定 order + 注册顺序排序，暴露阶段输入/输出和启用状态；先只注册本轮阶段。预留 200/400/500/700，但不实现自动驱动器。排序不得依赖 HashMap 遍历，不允许节点在执行中任意修改注册表。首轮不做 DAG 编排或任意脚本插件。

`RuntimeState` 至少包含真实参数、虚拟参数、motion 基准参数、Part 输入透明度、Model opacity、播放条目和事件游标、expression 混合状态、physics 粒子与插值缓存、pose 初始化状态。

Part opacity 必须按 Core 的实际层级语义进入几何/渲染结果，并验证与 Offscreen 组合时只应用正确次数。Model opacity 的应用位置对照 Framework 渲染器，不能直接猜成对最终图像乘一次 alpha。给 core 新增 typed runtime input，保留现有参数-only API 的默认效果。

预览需支持两种不同操作：

- `sample_motion(clip, time)`：无状态曲线采样，供曲线面板和快速检查，不宣称包含有历史依赖的物理、Pose fade 或多次激活事件。
- `advance(dt)` / `seek(t)`：完整状态预览。seek 以规定的初态和激活时间表重放到 t；向后定位不能直接把 physics 时间改成负数。

编辑器默认重放调度步长建议 1/60 秒；保留 physics 文件自己的 fps，缺 fps 时按传入子步工作。官方差分比较采用完全相同的初态、dt 序列和激活事件。末尾不足一个步长的处理固定，缓存快照必须落在统一时间格点，防止 seek 历史改变结果。

seek 保留从初态重放的基准路径，并已加入有内存上限的 checkpoints（默认 16 MiB，可禁用）。checkpoint 包含上述所有状态，不只粒子；仅存储标准 60 Hz 回放的整秒格点。缓存属于不可变文档快照的预览实例，初态、时间表或输入序列改变时整体失效，步进策略固定。命中后进度报告剩余回放步数，取消同时保留原播放状态与缓存。

播放事件按跨越的时间区间一次性产生，定义循环端点、零秒、重复同时间采样、大 dt 和 seek 的行为。默认 scrub 返回事件诊断但不触发播放副作用；需要重新发送事件须显式选择。暂停不推进淡入或物理；reset 重建完整初态。

多表情混合对照 `CubismExpressionMotionManager`，不能仅循环调用普通加乘赋值替代其切换语义。Motion 的混合队列至少支持编辑器验证所需的开始、停止、权重、交叉淡化；宿主的随机 idle/tap 策略后置。

## 7. 各格式的验收细节

| 格式 | 必须编辑/保存/导出的字段 | 必须验证的边界 |
| --- | --- | --- |
| exp3 | Type、FadeIn/Out、Parameters.Id/Value/Blend | 缺省 Add、缺省 fade、零秒、多表情交接、相同参数多模式 |
| motion3 | Version、Meta、Curves、UserData；三 Target、四 Segment、逐曲线 fade | restricted/unrestricted Bezier、边界连续性、循环 V2、不同 FPS、UTF-8 事件大小、计数重建 |
| physics3 | Version、Meta/Fps/EffectiveForces/PhysicsDictionary、rigs、Normalization、Input/Output/Vertices | fps 缺省、reset/stabilization、多 rig 顺序、输出索引、反射、权重、异常 dt 与长序列 |
| pose3 | Type、FadeInTime、Groups.Id/Link | 首项初始化、虚拟控制参数、零 fade、多项激活、无项激活、Link、嵌套 Part |
| cdi3 | Version、Parameters、ParameterGroups、Parts、CombinedParameters | 中文与重复显示名、组层级循环、缺失组、参数删除/重命名、组合顺序 |
| model3 | Moc/Textures/Physics/Pose/DisplayInfo/Expressions/Motions；Groups/Layout/HitAreas | 注册别名、motion group 顺序、相对路径、共享文件、Sound 与 UserData 依赖 |

Version/Type 以真实格式为准：不能因为扩展名含“3”就给 exp3/pose3 强加统一 Version。计数、字符串字节大小及输出 meta 从内容重算，不信任导入计数。

## 8. 完整资源包导出

示例（可选功能没有内容时，不生成虚假占位文件或悬空引用）：

```text
model/
  model.moc3
  model.model3.json
  model.physics3.json
  model.pose3.json
  model.cdi3.json
  textures/0.png
  expressions/<stable-id>.exp3.json
  motions/<stable-id>.motion3.json
  sounds/...                  # 仅导入/关联了 Sound 时
  userdata/...                # 仅存在关联时
  export-report.json
```

CDI 默认从编辑器数据生成；expression/motion 文件使用稳定身份命名，显示名放注册信息和 CDI，避免重命名引起路径碰撞。最终 model3 中 `DisplayInfo` 引用 cdi3，Physics/Pose 分别引用各自文件，Expressions 包含 Name/File，Motions 按组包含 File 及可选 fade/Sound。

发布流程：

1. 从同一已提交 revision 捕获文档、资源描述、runtime ID map。
2. 构建纯数据 `ExportPlan`：每个文件的相对路径、类型、来源对象、内容/哈希、引用边。
3. 校验目标 namespace 的 ID、所有 model3 引用及附件目标、路径唯一性、大小写碰撞、缺失资源；输出完整诊断后再发布。
4. 编码 MOC3 与全部 JSON；校验数字有限、格式计数、编码后可读。资源读取一次并校验哈希，验证和发布使用同一份 bytes。
5. 在临时目录执行整包验证。Framework 可用时加载并运行场景 probe；不可用时在报告中明确 unavailable，不能宣称 runtime passed。
6. 延用现有 stage/backup/rename/rollback 发布契约，一次替换整个目录；失败保留旧目标，报告准确的 durable/warnings 状态。

扩展现有 validator 为包级验证，同时保留 MOC3 structural/runtime 结果；分别报告 JSON、引用完整性、Framework loader、动态场景、渲染验证状态，不用一个布尔值混淆。

本轮为已有 Sound 和 UserData 提供受管资源复制与引用保存，不实现音频处理或完整 UserData 编辑器。它们不能继续引用导入源目录。UserData 内已知 Drawable ID 必须通过映射校验；无法验证的扩展字段必须明示限制。

导入策略采用“两阶段提交”：读/解析所有附件形成候选工程与诊断 → 通过结构验证后替换 session。附件损坏不得悄悄返回“完整成功”。可保留显式 recovery 导入模式，用于打开缺资源工程继续修复；严格导出仍阻止缺依赖发布。

## 9. 实施阶段与验收门槛

每个阶段应包含 Rust、Python、工程保存/重开和必要文档，避免先做大量不可使用的内部类型。

| 阶段 | 交付内容 | 完成门槛 |
| --- | --- | --- |
| P0 兼容基线 | 建立格式样本矩阵、Framework CPU 行为 probe 协议，锁定关键默认值/差异 | PartOpacity→虚拟参数→Pose、repeat 最大值、Model opacity、motion V2、physics fps 的参考输出可复现 |
| P1 文档与打包基础 | AnimationAssets、引用类型、v5、资源依赖、ExportPlan；先做 CDI + model3 完整路径 | 参数显示名/分组编辑可撤销保存；CDI 与 MOC3 ID 对齐；整包原子发布失败测试通过 |
| P2 调度与 Expression | RuntimeState、最小 scheduler、帧输入、预览快照与 exp3 CRUD/codec/混合 | 单/多表情、淡入淡出数值对齐；预览不改文档；export→Framework 可加载 |
| P3 Motion | typed tracks/segments、时间轴命令、motion3 codec、队列/事件、sample/advance/seek | 四类 segment、三类 Target 编码完成；支持部分的运行时行为对齐；后置 Model effect 映射明确诊断 |
| P4 Pose | pose3 编辑与 codec、虚拟参数、Part opacity 层级求值、Pose preview | motion 驱动换部件、Link 和淡入、嵌套 Part/Offscreen 场景对齐 |
| P5 Physics | rig 编辑与 codec、模拟/稳定化/重置、确定性重放 | 多 rig 长序列、不同 dt/fps、输出边界与参考数值对齐；反复 seek 结果一致 |
| P6 整体验收 | 全部资源联合导出、真实模型往返、wheel 场景、报告与用户文档 | 原目录移走后导出包独立可用；官方加载+行为+选定渲染场景通过，无静默资源丢失 |

P1 先做数据与交付骨架，P2 即开始调度，符合“调度器可先做”的范围。P3 的虚拟通道为 P4 做准备；在 Pose 未完成前，PartOpacity 的视觉效果不得标记为完成。P5 为主要数值兼容风险，不能压缩成“接入一个 JSON 文件”。

每阶段建议分成小 PR：领域/契约 → codec/持久化 → 编辑入口 → 预览与参考验证；分拆后的每个 PR 都要有明确可运行检查，不长时间留下假成功 API。具体工期在 P0 得到参考行为与最小 probe 构建结果后再估算。

## 10. 验证方案

现有 `tools/probes/project_runtime_probe.cpp` 验证的是 Core，不能证明 Expression/Motion/Physics/Pose 正确。新增独立 Framework 动画 probe，复用本地 SDK 定位和工具构建约定；尽量只链接必要 CPU 源码，检查实际构建依赖后确定，不把“无 GPU 构建”当已验证事实。

probe 输入为包路径、明确的初态、dt 序列、参数输入和动作/表情激活时间表；输出逐帧参数、虚拟控制值、Part opacity、Model opacity、事件、选定 drawable 数据。固定 SDK/Framework 来源哈希、模型和附件哈希、兼容配置与浮点容差。

| 验证层 | 必测内容 |
| --- | --- |
| 单格式 | 自建最小 fixture 的解析/编码/再解析语义相等；损坏计数、极端值、非法引用不崩溃 |
| 文档契约 | 所有新增对象的事务失败原子性、undo/redo、引用保护、handle 失效、保存重开、v1–v4 迁移 |
| 算法差分 | Framework 相同输入序列的表达式混合、Bezier/循环/事件、Pose fade、Physics 累积误差 |
| 组合行为 | Motion+Expression+Physics+Pose，确认顺序、基准参数保存与透明度只应用正确次数 |
| 编辑器时间 | 播放/暂停/reset、前后拖动、重复 seek、缓存失效、seek 期间取消、编辑后刷新 |
| 整包交付 | model3 引用遍历全存在；移走源目录仍可加载；同名/非 ASCII/路径碰撞；失败不破坏旧包 |
| 观察与渲染 | 已捕获动画快照直接渲染；选定帧对官方 Framework 的图像差分，重点 Pose/Offscreen/Model opacity |
| 性能 | 长 motion、多 rig、长时间 seek、history 内存上限；先记录基线，再按编辑交互目标设预算 |

synthetic fixtures 可纳入仓库；本地 SDK 与厂商样例继续按仓库既有方式存放，不把它们复制提交。无 SDK 环境跑纯 Rust/Python 契约；完整发布验收环境必须能运行官方 Framework 对照，未运行不可标为通过。

数值容差按参数范围、坐标单位与序列长度在 P0 建立；记录 max/mean 和首个分歧帧。不得用放宽容差掩盖错误执行顺序或状态重置。算法测试通过后再做选定渲染帧，而不是为每个 JSON 字段写像素测试。

## 11. 先行决策与待验证项

本规划的默认决策：physics3 属于完整导出；先 SDK 后 GUI；核心动画逻辑留在 Rust；保留现有无状态求值 API；新增有状态预览；写 v5 工程；默认 Cubism 5-r.5 MotionBehavior V2；导出可选文件以实际资产为准；模型注册信息与曲线内容分离。

执行中已确认的方案（2026-09-25，用户批准）：

- P0 的 CPU probe 在自己的可执行目标中提供空 `CubismRenderer::StaticRelease()`，只满足无图形资源情况下的后端清理链接点。Framework 正常初始化与销毁，第三方源码不修改；此 shim 不进入产品或 GPU probe。CPU 输出不作为渲染正确性的证明。
- 本地 Framework 的 `CubismJson::ParseNumeric` 不接受数字紧邻 `]` 等部分合法 JSON 写法。后续导出采用经过该 parser 实测的 JSON 编码约定；P0 保留合法但被拒绝的原始输入作为负控制，同时验证数值表示、终止符及读取数值是否准确。不能只凭换行样例成功就认定全部数字兼容，也不能修改参考 parser 来掩盖导出兼容问题。
- Repeat 基线同时记录 Framework 默认模型级 override 与关闭该 override、遵循 MOC repeat 标志两种配置。每份 trace 明示配置及模型原始标志；P0 不改变 Kasane 默认行为。默认 override 使 SDK 侧初始 repeat=false 生效，不能将这一宿主设置误认为 MOC 无循环参数。
- 2026-09-25 后续工作由主代理直接实施。阶段按验收结果推进；新的阻塞、兼容性冲突或需改变方案的问题先暂停，与用户讨论。

P0 必须解决、无需提前让用户猜测的技术问题：

- Part/Offscreen/Model opacity 在官方 Core+Framework 的准确应用路径及现有 Kasane 所需改动。
- repeat/clamp 的差异如何在新 runtime input 路径处理，确保不回归现有 keyform 与循环参数测试。
- Physics 允许范围、缺参数与退化 rig 的 Framework 行为；区分可恢复导入和不可执行资产。
- 真实 motion 文件的 Target 顺序和混合/循环边界；明确 canonical 编码策略与兼容差异。
- 独立 CPU probe 的构建范围和 SDK 源码身份；第三方目录可能有本地修改，因此目录版本名不能作为唯一来源标识。
- 初态、时间表与 checkpoints 的最小完整表示；先正确重放，再引入 seek 加速。

CPU 数值、打包路径和附件独立性，以及选定动画帧的 GPU 图像差分均已有可复现 probe。独立 GPU 验收结果见实施记录；发布操作自身未执行渲染验证，因此 `export-report.json` 仍明确为 `not_performed`。

## 12. 源码索引

- [现有架构](ARCHITECTURE.md)
- [当前导入](../modules/kasane-moc3/src/importer.rs)、[MOC3 artifact](../modules/kasane-moc3/src/types.rs)、[包发布](../modules/kasane-project/src/package.rs)
- [文档](../modules/kasane-core/src/document.rs)、[Part 求值](../modules/kasane-core/src/evaluation/parts.rs)、[preview](../modules/kasane-core/src/preview.rs)
- [Framework motion](../third_party/CubismSdkForNative-5-r.5/Framework/src/Motion/CubismMotion.cpp)、[expression manager](../third_party/CubismSdkForNative-5-r.5/Framework/src/Motion/CubismExpressionMotionManager.cpp)
- [Framework scheduler](../third_party/CubismSdkForNative-5-r.5/Framework/src/Motion/CubismUpdateScheduler.cpp)、[update order](../third_party/CubismSdkForNative-5-r.5/Framework/src/Motion/ICubismUpdater.hpp)
- [Framework physics](../third_party/CubismSdkForNative-5-r.5/Framework/src/Physics/CubismPhysics.cpp)、[pose](../third_party/CubismSdkForNative-5-r.5/Framework/src/Effect/CubismPose.cpp)
- [Framework CDI](../third_party/CubismSdkForNative-5-r.5/Framework/src/CubismCdiJson.cpp)、[model settings](../third_party/CubismSdkForNative-5-r.5/Framework/src/CubismModelSettingJson.cpp)
- [既有 probe 构建](../tools/probes/CMakeLists.txt)
