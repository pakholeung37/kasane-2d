# 渲染边界重构建议

状态：2026-09-23，基于当前未提交实现的设计评审；本文是建议，尚未实现。
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
编辑器和 runtime 采用同一个发布契约；Rust 内部借用帧或保留 Arc，避免逐提交深拷贝。
是否采用连续 position arena，应由测量决定；协议应允许借用切片，先不要强迫 core 重写求值器。

先使用可靠的粗粒度版本，再逐步增加 geometry/material/order/mask-input 版本。
版本必须来自实际发布的数据；不能把用户编辑 mesh_ids 直接当成最终 dirty 集合，因为 deformer、glue 和参数会影响其它网格。
无版本保证的外部 DrawableFrame 仍走完整验证和同步路径；不能为加速而相信调用方未证明的数据不变。
动态 positions 和数值有限性检查仍然保留。

### 3. LogicalFrame：表达合成语义和依赖

不用“带隐式 stack 的伪执行流”作为公共契约。采用显式 target 的有序 draw/composite 列表，并记录 mask/offscreen 输出的依赖。
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
源 ID 去重/排序需先用图像回归确认：当前 shader 使用 alpha 混合，重复源会改变覆盖率，不能擅自按集合消除重复项。

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
get_mesh_view/get_offscreen_texture 作为 Godot 调试接口保留，不进入共享 Renderer 契约。
Godot 的 pre/post draw、viewport 可见性和 pending_draws 策略属于宿主观察逻辑，不强制推广到其它后端；若以后统一，只暴露 submission token 与完成状态。

## 建议实施顺序

1. 固化当前图像与性能基线，补足 target 顺序、mask 源不可见、相机刷新、文档替换和动画排序的语义用例。
2. 在 render 内加入显式 target/item/dependency 表示，以现有 DrawableFrame 作为兼容入口；Godot 成为第一个消费者。旧 PreparedFrame 仅作为短期兼容外观，不维护两套独立语义。
3. 统一 mask bounds/attachment 描述和预算路径，将 consumer 物理复制策略下移到 Godot。
4. 建立持久 RenderPlanner、generation/帧/view 版本与可复用 scratch；先实现相机刷新不重新同步几何，再做分项 dirty 同步。
5. Godot 内部采用类型化输入，按 dirty 更新材质/节点顺序/mesh；runtime 复用 Arc 发布帧。不要同时重写 core evaluator、纹理 IO 和 backend。
6. 完成 Godot 回归后，再让下一 backend 消费已经验证的协议。本轮无需改动 wgpu。

## 验收目标与本次验证

正确性：保留当前 blend matrix、嵌套 offscreen、viewport-chain、mask、截图 ready 与生命周期测试；依赖图测试覆盖 composite 的生产者/消费者顺序、每次 destination 读取位置和帧间失效。
性能：release 同环境比较 flat 与 Ren offscreen；分测静止、纯相机、纯参数、appearance/order 和结构变更，不只测总体帧耗时。
分别记录 evaluate、prepare、resource sync、submit、端到端时间，以及分配数、顶点上传字节、mask 重绘、节点重排、surface 创建/resize 与显存。
无变化路径应无同步工作；纯相机应无顶点上传；静态拓扑不应逐帧重新扫描 indices/UV；稳定结构的规划 scratch 应复用容量。
全动画场景仍允许 O(V) 的求值和动态检查，不承诺总帧处理恒定时间。

本次执行 `cargo test -p kasane-render -p kasane-preview --locked`：15 个单元测试通过，doc tests 通过（无用例）。
未修改运行时代码，未运行新的 GPU/端到端 benchmark；上述性能问题来自代码路径检查，收益需按以上场景实测。
