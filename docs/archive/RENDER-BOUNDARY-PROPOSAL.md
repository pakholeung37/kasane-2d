# 渲染边界重构建议

状态：2026-09-23，第一阶段已接入 Godot 并完成回归与 benchmark。下面保留设计评审依据；实际完成范围见文末。提取阶段的性能基线在 4471373 保存。
范围：kasane-core 的帧输出、kasane-preview、kasane-render、kasane-render-godot 与 Godot 适配层。未审查 wgpu 实现。

## 判断

保留当前 crate 拆分，但不要把当前 PreparedFrame / RenderPass 固定为长期后端协议。
这次拆分成功隔离了 Godot 资源所有权，却尚未隔离 Godot 的资源组织策略，也尚未表达更新频率。
下一步最值得投入的是“持久场景结构 + 动态帧 + 视图布局 + 后端物理计划”，而不是继续拆文件或添加统一 Renderer trait。
性能最优不能由静态审查证明；以下给出可测量的优化目标，不承诺尚未测得的收益。

## 当前代码中的依据

1. `kasane-render/src/lib.rs` 的 RenderPass 明确规定 Composite 在子绘制之前发出；`plan.rs::prepare_passes` 实现了这一顺序。它适用于提前挂接 Godot Sprite，但不是生产者先完成、消费者再读取的执行顺序。新的执行式后端还得重新解释这套协议。
2. `MaskKey` 把 consumer 和 scale 放进共享资源身份；Godot masks.rs 的注释解释了 consumer 是为了视口依赖。逻辑 mask 的内容身份与后端需要的物理实例被混在一起。
3. `plan.rs::mask_reserved_bytes` 遍历 mask 顶点估算 bounds，Godot `update_mask_texture` 又通过节点快照重新计算。前者用 f64 尺度和至少 2 像素，后者用 f32 和至少 1 像素；估算与分配不是同一份描述。
4. 共享 planner 固定 4096 上限和每个 offscreen 8 字节/像素；这可以是当前 Godot 的保守策略，但不能作为所有后端的物理成本事实。
5. prepare_frame 每次全量校验拓扑、建立字符串索引和多份集合，再生成指令。借用字符串减少了字符串复制，并没有消除容器分配；mask key / consumer 仍会复制字符串。
6. backend 每次转换所有顶点、更新材质、重新排序场景节点。MeshView 会比较顶点并跳过未变化的 GPU 上传，但比较之前已发生 PackedArray -> Vec 的转换与校验。删除检测还有逐项扫描 frame 的二次复杂度路径。
7. document_preview 的相机刷新复用 core 的 Arc 帧缓存，这是好的；但渲染提交仍全量执行。runtime 路径还会在 submit_frame 和相机刷新复制包含 positions 的 DrawableFrame。
8. core 已有 PreparedEvaluation、共享 UV/indices，以及 PreviewState 的 generation/document/preview 三元缓存键。这些值得延续；不能仅以 DrawableFrame.source_revision 判断动画帧是否变化。

这些问题不都由本次提取引入，但新协议若原样固化，会把它们带入下一后端。

## 建议的四层数据边界

```text
Document / runtime source
          |
          v
SceneStructure + FrameState
          |        + ViewConfig
          v
RenderPlanner -> LogicalFrame
          |
          v
Backend lowering -> PhysicalPlan -> resource sync / execute
          |
          v
Host presentation / completion observation
```

### 1. 持久 SceneStructure

保存 mesh/texture/group 的稠密句柄、UV/indices、静态 mask 关系和组归属。
字符串 ID 仅用于入口解析、编辑操作和诊断；每帧数组访问使用类型化索引。
第一版可采用“scene generation + 全量重建稠密索引”，无需立刻实现复杂的槽位回收器。
索引不能跨 generation 使用；拓扑替换、删除和 undo/redo 都必须失效。

在结构发布时校验索引、UV 长度、引用和层级。保留已有 Arc 静态几何，避免为接口整洁再复制一份。
绘制顺序、可见性、透明度及 blend 不默认静态：动画可改变它们。

### 2. 动态 FrameState

保存 positions、appearance、visibility、当前绘制顺序，以及相对于结构版本的更新标识。
编辑器和 runtime 采用同一个发布契约；Rust 内部优先借用帧，避免逐提交深拷贝。不能让 backend 为相机刷新长期保留编辑器上一帧的强 Arc：这会阻止 PreviewState 回收缓冲。runtime 是否传入拥有所有权的 Arc，应由其发布者的回收协议决定。
是否采用连续 position arena，应由测量决定；协议应允许借用切片，先不要强迫 core 重写求值器。

先使用可靠的粗粒度版本，再逐步增加 geometry/material/order/mask-input 版本。
版本必须来自实际发布的数据；不能把用户编辑 mesh_ids 直接当成最终 dirty 集合，因为 deformer、glue 和参数会影响其它网格。
无版本保证的外部 DrawableFrame 仍走完整验证和同步路径；不能为加速而相信调用方未证明的数据不变。
动态 positions 和数值有限性检查仍然保留。

### 3. LogicalFrame：表达合成语义和依赖

建议采用显式 target 的有序 draw/composite 列表，并记录 mask/offscreen 输出的依赖。Begin/End 流本身并不是错误的抽象，也可以编译成正确的执行序列；当前问题是把 Godot 的提前挂接顺序称为后端通用的执行顺序。选择 target 列表是为了持久缓存、依赖检查与增量同步，不代表换一种 IR 就能提高帧率。
每个 target 内保留原始混合顺序；跨 target 按依赖安排生产者。
现有模型是嵌套 offscreen + mask，第一版用数组和依赖列表即可，不需要通用 frame-graph 框架。

示例：root 的列表为 Draw(A), Composite(G), Draw(B)；G 的列表为 Draw(C)。
G 的内容必须在 root 读取 G 前完成；root 内 A、G、B 的混合顺序不能改变。
Godot lowering 负责场景树和 viewport parent 设置；执行式 lowering 负责按依赖编码命令。

destination read 必须表示“该 target 在这个 draw/composite 之前的内容”，不能仅表达“某 ID 需要一份 destination”。
可以在有序 item 上记录 read-before-write 语义；后端决定复制、分段或其它实现。
不要把不同 draw 重排到一起来省 draw call，除非证明不改变混合结果。

逻辑 MaskId 表达源集合及采样语义；consumer 是依赖边，分辨率是布局结果。
Godot 可在 lowering 中按 consumer 复制物理 mask；其它后端可以复用同一逻辑结果。
源 ID 去重/排序需先明确兼容契约：Document 的 mesh masks 拒绝重复，但 offscreen masks 和外部帧允许重复；当前 Godot 的源节点以 ID 为键，重复 ID 实际只画一次。第二轮像素测试确认了这一点，修正了初稿“重复源会重复混合”的推断。第一版保持源列表的稳定顺序，不为提高共享率而擅自排序；可在入口保留首次出现顺序去重，但需要将这一兼容规则写入契约。

坐标与颜色语义也要成为契约：模型坐标、canvas 像素、target 像素的变换，UV 原点、绕序、mask bounds、straight/premultiplied alpha 和颜色空间。
第一阶段显式记录并保持现状，不夹带视觉修正。Godot 的 UV 翻转、绕序和顶点格式转换留在适配层。

### 4. PhysicalPlan：布局结果驱动真正分配

共享层计算或缓存源几何 bounds；mask 尺寸、采样变换与 attachment 描述计算一次，后端直接消费。
渲染质量策略可以共享，例如 padding 和目标采样密度；纹理格式、物理复制、最大尺寸与常驻资源由后端策略决定。

保留 512 MiB 作为可配置应用预算默认值，但用 lowering 后的实际 attachment 描述计费。
区分逻辑总量、同时存活量、池内常驻量和提交时旧新资源重叠；预算失败应发生在资源分配之前。
当前 Godot 可保守预留 destination 存储，不能要求每个后端都按同一倍数收费。
第一阶段继续每组一个持久 surface；等依赖及生命周期明确后，再评估目标复用、mask atlas 和局部 destination copy。
这些优化具有不同失效/采样成本，不适合先写入公共接口。

## 刷新与失效规则

| 变化 | 必要工作 | 应避免的工作 |
| --- | --- | --- |
| 无变化 | 按宿主需要展示已同步资源 | 校验、排序、重新打包顶点和同步节点 |
| 相机/窗口 | target 布局、变换、受影响 mask 密度 | 模型顶点上传和拓扑校验 |
| positions | 动态几何、bounds、依赖它的 mask | UV/index 重建、无关材质更新 |
| appearance | 对应材质与可见性，必要的 destination 策略 | 无关顶点上传 |
| order | target 内顺序、读目标位置 | 几何和纹理重建 |
| topology/mask 关系 | 重编译受影响结构，首版允许全量重建 | 无条件重新解码纹理 |
| texture 内容 | 纹理更新及受影响 mask | 无关 mesh 重建 |

相机变化有时需要重新渲染 mask 或重分配 surface，因此目标是零几何上传，而非保证零 GPU 工作。
可见性剔除必须保留 mask 所需的源：不可见 drawable 仍可能参与遮罩；从最终输出反向求需求集合。

## crate 与 API 的职责

- kasane-core：求值和不可变发布，保留现有静态几何与帧缓存；不拥有 GPU 资源。
- kasane-render：场景编译、句柄解析、逻辑依赖、共享几何 bounds/质量策略；不规定 Godot consumer 分配策略。
- kasane-render-godot：Godot lowering、节点/材质/纹理/viewport 缓存和增量同步。
- kasane-preview：项目资源校验与刷新编排；资源准备成功和画面完成是两个状态。其现有小型 AssetResolver 可保留，不扩张成所有 backend 的万能接口。
- kasane-godot：GDExtension API、文档信号、资源入口和宿主展示。

内部用类型化 Result/Status/Stats；Dictionary 转换留在 GDExtension 方法。
MeshView 的公开检查接口仍需校验，renderer 内部可走已验证输入路径，避免 Rust -> PackedArray -> Vec -> Dictionary 往返。
get_mesh_view/get_offscreen_texture 作为 Godot 专用接口保留，不进入共享 Renderer 契约。get_mesh_view 不只是调试接口：SelectionOverlay 当前依赖它读取顶点。未来更换 backend 前，要给 overlay 提供只读的 preview-local geometry 查询，Godot 节点访问仍可作为兼容接口。
Godot 的 pre/post draw、viewport 可见性和 pending_draws 策略属于宿主观察逻辑，不强制推广到其它后端；若以后统一，只暴露 submission token 与完成状态。

## 建议实施顺序

1. 固化当前图像与性能基线，补足 target 顺序、mask 源不可见、相机刷新、文档替换和动画排序的语义用例。
2. 在 render 内加入显式 target/item/dependency 表示，以现有 DrawableFrame 作为兼容入口；Godot 成为第一个消费者。旧 PreparedFrame 仅作为短期兼容外观，不维护两套独立语义。
3. 统一 mask bounds/attachment 描述和预算路径，将 consumer 物理复制策略下移到 Godot。
4. 建立持久 RenderPlanner、generation/帧/view 版本与可复用 scratch；先实现相机刷新不重新同步几何，再做分项 dirty 同步。
5. Godot 内部采用类型化输入，按 dirty 更新材质/节点顺序/mesh；runtime 的拥有所有权提交接口单独处理，不影响编辑器帧回收。不要同时重写 core evaluator、纹理 IO 和 backend。
6. 完成 Godot 回归后，再让下一 backend 消费已经验证的协议。本轮无需改动 wgpu。

## 验收目标与本次验证

正确性：保留当前 blend matrix、嵌套 offscreen、viewport-chain、mask、截图 ready 与生命周期测试；依赖图测试覆盖 composite 的生产者/消费者顺序、每次 destination 读取位置和帧间失效。
性能：release 同环境比较 flat 与 Ren offscreen；分测静止、纯相机、纯参数、appearance/order 和结构变更，不只测总体帧耗时。
分别记录 evaluate、prepare、resource sync、submit、端到端时间，以及分配数、顶点上传字节、mask 重绘、节点重排、surface 创建/resize 与显存。
无变化路径应无同步工作；纯相机应无顶点上传；静态拓扑不应逐帧重新扫描 indices/UV；稳定结构的规划 scratch 应复用容量。
全动画场景仍允许 O(V) 的求值和动态检查，不承诺总帧处理恒定时间。

第一轮执行 `cargo test -p kasane-render -p kasane-preview --locked`：15 个单元测试通过，doc tests 通过（无用例）。后续实际 benchmark 与第二轮验证见下文。未修改运行时代码，新架构的收益仍需在实现后实测。

## 第二轮验证：必须进入方案的约束

### 帧所有权和版本

`PreviewState::invalidate` 用 Arc::try_unwrap 把已发布帧归还给 output；FrameEvaluator 在成功时与 scratch 交换，保持失败不污染输出。backend 持有旧帧会让这个回收失败；即使随后释放旧帧，也不会自动归还给 evaluator。

新增 evaluation_workspace_tests 的实验：100 个静态网格，预热 8 次，再发布 12 次参数变化。
及时释放快照记录 36 次分配；一直持有上一帧直到下一帧发布后记录 8,544 次分配。这是当前分配器探针的 alloc 计数，不是耗时倍数，也不包含单独统计的 realloc 次数。

因此编辑器路径采用：

1. core/preview 持有帧，backend 只在同步调用期间借用。
2. backend 留下必要的几何资源、bounds、appearance 小缓存和版本标识，不再保留整个 DrawableFrame。
3. 相机更新只消费已经同步的资源及缓存布局，不需要复制或重新求值一帧。
4. 加入独立的发布标识（source identity/generation + 成功发布序号），而不是使用 source_revision 或可复用的内存地址。参数变化可发布 source_revision 相同但内容不同的帧，测试已断言这一点。
5. 不要把 metadata 修改也强行变成新渲染帧；保留当前 PreviewState 对等价求值输入的复用。

第一版不要求 core 立刻提供每网格 revision。新帧可先比较 O(N+E) 的小字段并执行必要的 O(V) 动态几何工作；相机快路径先落地。静态拓扑的验证缓存必须持有对应 Arc 几何并校验顶点数，不能仅凭调用方传来的数字版本跳过校验。

### 依赖图必须区分原始网格输入与 target 输出

Godot mask.gdshader 读取纹理原始 alpha；masks.rs 绑定源 mesh/texture，不使用源 drawable 的 opacity、multiply/screen、inverted 或它自己的 mask 结果。
core 会为因 opacity=0 而不可见的网格保留几何；因 enabled=false 被禁用的网格则可能输出退化到原点的几何，不能把这两种不可见情况混为一谈。

测试构造：G 内有源网格 S 和可见网格 D，G.masks=[S]；S.opacity=0，S 自身又被零三角形网格遮罩。
半透明 atlas 的 D 和原始 S 各贡献约 0.5 alpha，最终输出约 0.25，GPU 像素检查通过。

正确依赖是 `Geometry(S)/Texture(S) -> Mask(S) -> Composite(G)`，以及 `Draw(D) -> Target(G) -> Composite(G)`。
不是 `Target(G) -> Mask(S)`；后者会产生并不存在的循环。mask 源自己的 mask 不应递归展开为当前 mask 的输入。

这也意味着未来的可见性裁剪至少区分 color draw、mask source geometry、composite 三类需求，不能过滤掉所有 visible=false 的输入。

### 缓存键要分离几何、纹理内容和布局

第二轮 GPU 测试在不改变 ImageTexture 实例和尺寸的情况下调用 update，再显式 refresh_geometry；输出从约 0.25 alpha 变成约 0.5625。这证明纹理 identity/dimensions 不足以认证缓存。

需要区分：

- GeometryKey：scene generation + UV/index 所有权/版本 + 顶点数；不含 texture handle。
- TextureBinding：native handle 和 sampler。
- TextureContentEpoch：由资源入口报告内容更新；未提供更新保证的外部纹理，显式 refresh 必须保守失效相关 mask，不能静默假设未变化。
- MaskContent：源几何、UV、纹理内容；consumer opacity/inverted 作用于消费端，不是 mask 内容。
- MaskLayout：canvas 映射、padding、采样尺度、extent。布局变了可以重绘或 resize，但不需要改变逻辑 MaskId。
- Godot 物理实例：逻辑 mask + consumer 的宿主身份/生命周期。换 viewport 后即使 frame token 相同，也必须更新依赖和观察状态。

当前 MaskKey 把精确 scale bits 放进身份，scale 变化会创建新 mask viewport 再 queue_free 旧资源。新实现应保持资源身份稳定，尺寸/变换作为可更新状态；峰值预算必须考虑延迟释放。
同尺寸 texture handle 换绑在当前实现中会增加一次 mesh surface creation，GPU 探针已记录。新设计应更新绑定而不重建几何；现有 lifecycle_boundary 中对此次重建的计数断言需要在该优化实现时有意更新，不能误当作必须保持的产品行为。

### 持久缓存不是四份场景副本

初稿四层描述的是职责和更新频率，不要求四套完整 owned 数据：

- RenderScene 只拥有稠密索引、静态 Arc、mask 引用、target 归属，以及验证记录。
- FrameInput 借用 DrawableFrame 的 positions/appearance；兼容入口继续接受现有帧格式。
- LogicalFrame 借用场景句柄并复用 target/items scratch；绘制顺序每帧允许变化，target 成员变化要触发结构失效。
- PhysicalPlan 保留 attachment 描述、backend 依赖和更新集合，不复制所有顶点。

对于未声明可信版本的外部帧，先做校验/结构匹配；只有由 planner 自己构造且字段受控的已验证对象才能进入快路径。当前 PreparedFrame 字段公开，不应直接当作“已验证证明”。
第一步可以完全不修改 DrawableFrame 的公开字段，也不用立即引入 trait object、资源 ECS、bindless 或通用 DAG 调度器。

### 错误与宿主边界

新帧在校验、引用解析和预算通过前，不发布新 token、不把资源缓存标为已同步。Godot 执行失败的现有处理有清空资源和保留旧图像两类路径；先保持调用方的失败/ready 行为，不在本轮偷偷改变“错误时显示旧图还是空图”的产品策略。
不得因有上一帧图像就报告本次提交完成。跨 viewport、隐藏再显示、删除 surface 后重挂仍遵循宿主的 completion 状态机。
Godot 的节点归属与 queue_free 不应进入共享层；删除父 surface 前对存活 mesh/composite 的 reparent 必须继续保留，不能用简单 cache.retain 取代。

### 测量范围与实施门槛

重构前 c21c111 对比 de62a6b，两负载各重复 3 次：fixture CPU mean +3.33%，frame mean +0.27%；Ren CPU mean 约不变，frame mean -0.64%。两者通过现有 5% 门槛，不能据此证明下一方案会更快。
原始记录见 [基线结果](../benchmarks/kasane-preview/results/20260923-backend-extraction/summary.md)。

benchmark 调用 `set_preview_values`，其返回时执行 inspection::get_frame，把 positions/UV/indices 等转换为 Dictionary。因此 refresh_cpu_ms 包含求值、渲染同步和整帧 API 返回值构造；frame_ms 还包含宿主调度等待，不是纯 GPU 时间。
后续保留这个端到端门槛，另外增加轻量 set_preview_parameter 路径或内部阶段计时；不能换掉旧测量入口后把差值全部归功于 renderer。
相机 GPU 上传为零在当前实现已经成立，本轮 GPU 测试也确认；真正新增的目标是减少相机路径 CPU 工作，并补计 prepare/转换/节点同步次数。

第二轮验证命令：

```sh
cargo test -p kasane-core --test evaluation_workspace_tests --test preview_state_tests --locked -- --nocapture
cargo test -p kasane-render -p kasane-preview --locked
python3 tools/validate_render_boundary.py
```

结果：core 两组 12 个测试通过（含新增所有权探针）；render/preview 15 个测试通过；GPU preview cache 23 项、surface lifecycle 106 项、新边界契约 25 项检查通过，destination-copy 正负对照通过。
GPU 脚本用当前 Release 库，在独立临时项目运行，不改用户正在打开的编辑器。报告位于 target/render-boundary-review/report.json。
这些验证锁定了当前行为和重构必须满足的条件；显式 target 计划尚未实现，因此没有声称新旧调度器已做差分验证或新架构已通过性能验收。

结论：保留总体方向，但将第一轮实现收敛到“可持久的 target/mask 描述 + 不持有整帧的 Godot 同步状态 + 独立 view 更新”。细粒度 core dirty propagation、runtime 发布器重写、资源池复用留到后续独立测量；不要一次替换整条求值链。


## 第一阶段实现与验收

已实现：

- `ScenePlan` 持久化 typed mesh/target/mask 索引、target 内顺序、原始 mask 输入和 bounds。模型提交先完整验证，结构未变时复用描述和容器；不保留 frame Arc、positions 或纹理句柄。
- Godot 从 `ScenePlan` 分别同步 target 顺序和 viewport 依赖。旧 `prepare_frame` 从同一描述生成兼容流，避免两套独立的层级解释器；未改动 wgpu 实现。
- Godot `ViewLayout` 统一 mask extent、采样变换和分配前预算。mask 原始 bounds 每源每帧只扫描一次，backend 不再从 MeshView 抓取顶点重新计算 bounds。物理 mask key 不包含精确 scale bits；跨 consumer 或跨最低模型密度策略仍可产生不同实例。
- preview 的 process 改走 `update_view`，不查项目纹理、不重校验模型、不转换顶点、不重排颜色节点。runtime 不再复制并缓存 DrawableFrame；相机更新直接使用同步后的资源。
- 无效视图不认证旧图完成；恢复视图可以继续更新。若模型提交先被视图拒绝，后续相机变化会重试 Document 提交，而不是使用不完整的同步状态。
- MeshKey 不再包含纹理身份，换绑纹理更新 native binding，不重建 mesh surface。新增四个统计量用于检查模型同步与视图更新的边界。

验收：新增 5 个 ScenePlan 契约测试覆盖顺序、raw mask 依赖、跨 consumer 逻辑共享、重复源、结构替换、失败前置验证、静态容器复用及帧所有权。core/render/preview Rust 测试、renderer Clippy 通过；GPU 边界 163 项（23+106+34）及 destination-copy 正负对照、Godot 集成 188 项、官方 GPU 图像对照 84 项通过。

[本轮 benchmark](../benchmarks/kasane-preview/results/20260923-scene-plan/summary.md)：相对同场重新运行的 de62a6b，fixture CPU mean -2.80%、frame mean +0.11%；Ren CPU mean -2.46%、CPU p95 -6.64%、frame mean -2.37%。两组均通过 5% 门槛，纹理显存和主要资源计数一致。相机的优化由零模型提交/零 CPU 几何同步计数验证；没有把参数 benchmark 的小幅变化冒充相机专用耗时测量。

仍未实现的后续项：core 每网格 dirty/version 发布、模型帧提交中的拓扑验证缓存、材质/颜色节点的细粒度差分同步、内部 MeshView 类型化上传接口、物理池与延迟释放峰值的严格总预算、非 Godot backend 的迁移。`ScenePlan` 的 unversioned 输入仍全量验证，`ViewLayout` 仍有小容器分配；这轮不声称模型动画路径已做到零分配或全局最优。


## 第二阶段：模型提交的增量同步

- `ScenePlan` 的私有 validator 按 UV/索引 Arc 身份和顶点数缓存拓扑校验。持有静态 Arc 防止地址复用，并让 `Arc::make_mut` 自动分离；不依赖 document revision、不持有整帧或动态顶点。动态坐标、画布、材质、引用和命令关系仍每帧检查。无效帧不会发布逻辑状态；其中单独通过校验的静态拓扑可安全缓存。
- Godot 缓存 drawable 材质输入值和 native texture 身份，未变化时跳过 shader/material 设置。遮罩绑定仍独立更新；同一纹理对象原地改像素后的显式模型刷新仍重绘遮罩。offscreen 材质尚未采用这一优化。
- 节点排序按实际节点身份、所属 target、目标颜色读取标志构造可复用的紧凑序列。序列相同则跳过 reparent/move_child，资源重建、draw order、target 或扩展混合切换会触发同步；不使用可能碰撞的 hash 代替相等比较。新增 `order_syncs` / `material_syncs` 统计。
- 新缓存属于 renderer 拥有的资源状态。外部代码可以读取 MeshView；直接修改 backend 管理的材质和父子顺序不属于提交契约。

后续仍包括 core 每网格 dirty/version 发布、内部 MeshView 类型化上传接口（现有热路径仍有 PackedArray 往返与重复检查）、offscreen 材质增量同步、小容器分配及延迟释放峰值预算。这轮没有将模型帧路径描述为零分配或全局最优。


[第二阶段实测](../benchmarks/kasane-preview/results/20260923-incremental-sync/summary.md)：fixture CPU mean -0.83%、frame mean +1.50%；Ren CPU mean -9.70%、CPU p95 -7.18%、frame mean -4.03%。均通过既有 5% 门槛，资源数量和 GPU 上传次数一致。Ren 的 121 次提交仅同步节点顺序 1 次、drawable 材质 221 次。

第二阶段验收：77 个 Rust 测试、renderer Clippy、174 项渲染边界检查及目标颜色读取正负对照、188 项 Godot 集成检查、84 项 GPU 图像对照通过。新增契约覆盖未变模型、透明度变化和绘制顺序变化的差分同步。
