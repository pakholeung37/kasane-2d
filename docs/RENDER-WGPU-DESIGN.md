# `kasane-render-wgpu` 迁移架构

状态：2026-09-23，WGPU 后端已能由独立离屏宿主实际出图；正式应用宿主尚待迁移。本文接续 [渲染边界方案](RENDER-BOUNDARY-PROPOSAL.md) 已落地的 `ScenePlan`。

## 现状与迁移决定

- `kasane-render::ScenePlan` 已提供稠密 `MeshId` / `TargetId` / `MaskId`、target 内有序 `Draw` / `Composite`、原始 mask 源、活动 target 和 mask bounds。`ScenePlan::update` 校验外部 `DrawableFrame`，不保留整帧。
- Godot 后端直接消费 `ScenePlan`。新的 `WgpuRenderer` 也从 `ScenePlan` 获取目标顺序、遮罩与目标读取关系，并拥有持续复用的 GPU 资源。旧的 `prepare_frame`/`WgpuBasicRenderer` API 仍在同一 crate 中供现有调用者过渡；新入口不解析旧 `RenderPass`。
- WGPU 已有真实设备上的像素测试与独立离屏宿主。Godot 现在只用于参考图像对照，未来应用宿主不会使用 Godot。

**决定：以 `ScenePlan` 为新 WGPU 入口的场景协议。** 旧实现的 WGSL 混合公式、坐标换算和测试用例用作迁移线索。`WgpuBasicRenderer`、`WgpuFramePlanner`、外置 attachment pool 和 `PreparedFrame` 公共 API 暂留供现有调用者过渡，不作为新应用宿主的入口。

## 分层与所有权

```text
kasane-core                  DrawableFrame（本次调用内借用）
kasane-render                ScenePlan（逻辑 target、顺序、mask、校验）
kasane-render-wgpu           WgpuRenderer
  ├─ Lowering / PhysicalPlan  附件、依赖、读目标分段、预算
  ├─ ResourceCache            mesh buffer、bind group、mask、target、snapshot
  └─ Encoder                  按目标顺序编码到宿主 CommandEncoder
宿主                         Device / Queue、源纹理、输出 view、提交和完成观察
```

`WgpuRenderer` 持有 `ScenePlan` 和属于一个 `wgpu::Device` 的资源；设备丢失后宿主重建 renderer。宿主继续负责 adapter、窗口 surface、图片解码/上传和输出目标。后端提供可单独运行的离屏示例/测试宿主，保证 crate 不只是一个未接线的库。

当前的最小调用形态（省略错误处理与创建代码）：

```rust
renderer.sync_model(&device, &frame, &textures)?;   // 模型提交，不借用帧
renderer.update_view(&device, view_config)?;       // 后续相机变化时单独调用
renderer.encode(WgpuEncodeTarget {
    device: &device, queue: &queue, encoder: &mut encoder,
    output: &output_view, output_mode: WgpuOutputMode::Replace,
}, &textures)?;                                    // 不调用 queue.submit
queue.submit([encoder.finish()]);                   // 宿主决定提交和观察完成
```

模型提交和视图更新分开。`sync_model` 验证并复制渲染所需的动态数据、拓扑和外观；返回后调用者可释放帧。纯相机/窗口变化不重新求值或读取调用者的旧帧，稳定模型在 `encode` 时不重新上传模型顶点。输入纹理由宿主提供借用的 view、尺寸和可选内容版本；没有版本的纹理会保守地重绘相关 mask。相同 view 原地更新像素时，宿主应递增版本。renderer 不持有上游帧的 `Arc`，以保留 `PreviewState` 的帧缓冲回收路径。当前实现仍在 `encode` 时同步 GPU 缓存，不在 `sync_model` 时上传。

宿主输出只要求可渲染的 view。后端始终先绘制到自己拥有的主颜色 target，再将它绘制到宿主输出；这样主 target 的扩展混合无需宿主提供 `COPY_SRC` texture。输出配置显式选择覆盖输出或按预乘 alpha 叠加到已有内容。首次实现允许这一次额外的全屏 pass，待正确性和成本测量后再考虑无 destination read 时的直绘快路。输出格式与内部工作格式分开协商，明确 alpha、sRGB 解码/编码与采样规则；格式选择必须经过像素对照，不能仅凭旧 shader 推断 Godot 等价。

### 新应用宿主的接入边界

当前 `kasane-godot` 预览返回 Godot `Texture2D`/mesh view，并由 Godot 的 viewport、截图 ready 状态机和选区覆盖层展示结果。正式迁移需要一个新的、直接持有 WGPU device/queue/surface 的应用宿主，接入模型提交、相机更新、源纹理生命周期、窗口 resize、预览展示与截图完成观察；选区覆盖层需要与后端无关的已求值几何查询。逐帧 GPU→CPU 读回再上传只用于测试/诊断。`examples/offscreen.rs` 是验证渲染正确性的最小独立宿主，Godot 后端只保留作迁移期的参考图像对照。

## 一帧的处理顺序

1. `ScenePlan::update` 校验帧、解析逻辑 ID 和有序 target。静态 UV/索引依靠其 validator 的 Arc 身份与顶点数缓存；动态位置及外部关系每帧仍校验。无效帧不产生新提交标识。
2. WGPU lowering 从 `ScenePlan` 和 view 生成物理描述：主/子 target 尺寸、实际格式、mask 布局、destination snapshot 需求、设备尺寸限制和显存预算。资源分配前完成所有可预见的 CPU 拒绝。若布局失败，当前 GPU 画面和最后成功的提交状态仍有效；`ScenePlan` 的候选发布需要暂存/提交机制，避免半更新状态被后续视图刷新误用。
3. 同步缓存。`MeshId` 配合场景 generation 定位 mesh；拓扑变更更新 UV/索引，位置变更更新动态 vertex buffer，外观/纹理变更更新 uniform/binding。camera 变换放在 pass uniform，不烘进每个顶点。mesh 缓存的 buffer 容量按需增长，删除对象及时回收。纹理内容、mask 内容与 mask 布局使用不同失效键。
4. 编码 mask。输入是源 mesh 的原始几何、UV 和源纹理 alpha，忽略其可见性、opacity、颜色、自己被遮罩后的结果及所在 color target。相同逻辑 `MaskId` 和相同物理布局可共享 attachment；若 mesh 与 composite 需要不同采样密度，保留两个物理实例。每次需要更新时清透明背景后重新绘制。
5. 从子 target 到父 target 编码颜色。每个 target 内严格遵守 `TargetItem` 顺序。活动 child 完成后才能在父 target 的对应位置 composite；父级 opacity/颜色/mask 仅在 composite 时作用一次。不可见 color draw 可以跳过，但仍被 mask 使用的源不能跳过。
6. 某个 draw/composite 读取 destination 时，先结束当前 render pass，将**该 target 此刻**的颜色复制到可采样 snapshot，再开启下一 pass 执行该 item。每次读取都重新复制；不得同时采样与写入同一 attachment，也不能将不同读取位置合并成一张旧快照。
7. 将内部主 target 合成到宿主输出。`encode` 返回本次 submission token、资源和同步统计；宿主 `queue.submit` 后负责完成/读回观察。编码成功不表示 GPU 已完成。

## 资源与语义约束

| 资源 | 身份与失效 | 第一版策略 |
| --- | --- | --- |
| mesh GPU buffer | scene generation + `MeshId`；静态拓扑、动态位置分别比较 | 保留容量，模型帧按需上传；相机变化零几何上传 |
| 源纹理 binding | 宿主 view/sampler 身份 | 换绑只重建 binding；纹理内容版本另外触发 mask 重绘 |
| 子颜色 target | `TargetId` + 工作格式；尺寸是可变布局 | 活动 target 各持有一个透明 attachment |
| mask | `MaskId` + 采样密度类别；bounds/extent 是可变布局 | 保持首次出现顺序去重；共享兼容布局的物理 mask |
| destination snapshot | 实际读取所在的 `TargetId` + 尺寸/格式 | 每个需读取的 target 保留一份，**每次读取前**更新内容 |
| pipeline | 工作/输出格式、blend 类型、mask 和 destination 变体 | 创建设备资源时缓存；不按 drawable 创建 |

当前预算按附件的 8 位 RGBA 实际尺寸、去重后的 mask 与 destination snapshot 计费，默认上限 512 MiB；同时检查 `Device::limits` 中的纹理及 mesh buffer 尺寸，并检查输出格式的保证用法与混合能力。仍需补齐 resize 时新旧附件重叠峰值以及全部 GPU buffer 的内存统计。暂不引入通用 frame graph、mask atlas、bindless 或跨 target 生命周期复用。

坐标契约保持模型 Y 向上、canvas/target Y 向下，源 UV 的翻转与三角形绕序由 WGPU 边界统一处理；mask 采样坐标始终为 canvas 像素。颜色缓存使用预乘 alpha 进行普通合成；固定 Additive/Multiplicative、`raw_blend_mode` 和 offscreen `blend_mode` 按现有 Godot/官方对照用例逐项验收。工作格式、纹理色彩空间与混合方程须在 GPU 读回下确认，不能把旧 WGSL 的存在当作正确性证据。

## 实施次序与验收

1. **最小纵切**：新 `WgpuRenderer` 直接消费 `ScenePlan`；离屏宿主上传纹理、提交一个平面 normal 场景、读回 RGBA。覆盖纹理方向、透明背景、顺序、相机和窗口 resize。此时就建立真实 GPU 测试入口与失败报告。
2. **缓存与遮罩**：持久 mesh buffer、raw alpha mask、反向 mask、重复源和不可见源；验证纹理原地更新、mask scale 和仅相机变化。用统计断言相机帧零顶点上传、稳定拓扑不重建 index/UV。
3. **离屏与混合**：嵌套 target、父子合成、固定混合；再实现逐 item destination snapshot 和全部扩展 blend。加入“普通 sibling → destination read → 下一 sibling”、连续两次 destination read、masked offscreen 和透明边界用例。
4. **宿主与生命周期**：外部 view 输出、设备/尺寸/模型替换、错误后恢复、提交完成与读回。提供 crate 内可运行示例；统计 buffer/attachment 创建、resize、上传字节、mask redraw、pass 数、CPU prepare/encode 时间和显存估算。
5. **应用迁移**：确认正式展示路径后，将编辑器预览、选区覆盖层与截图观察接到 WGPU 输出；用同一 fixture 比对 Godot 旧路径，再移除应用对 Godot 渲染资源的依赖。此步需要独立的端到端性能和生命周期验收。

每阶段的 GPU 图像与现有 Godot 图像及可用的官方 Framework 参考比较，遵循 [统一验收规则](VALIDATION.md#4-gpu-图像对照) 的确定区域 `2/255`、全图 MAE `0.005` 与大误差像素 `1%` 门槛；对颜色空间造成的系统差异先定位根因，不能放宽阈值掩盖。设备不可用的机器可运行纯 CPU 契约测试，但报告 GPU 项为 `not_run`，不宣称 WGPU 已可用。

`render-wgpu` 可用的完成标准：平面、mask、嵌套 offscreen、目标颜色读取及混合矩阵实际 GPU 渲染通过；增量刷新与资源生命周期有统计和测试；外部宿主可仅通过公开 API 创建、提交、展示和销毁 renderer。应用完成迁移的标准另含第 5 步的预览、覆盖层、截图和端到端验收。

## 当前实现与验收记录

- 将原先集中在 `lib.rs` 的实现拆为 `api`（公开数据类型）、`resources`（附件池）、`pipeline`（管线与基础渲染器）、`encoding`（场景命令编码）、`geometry`（顶点及坐标变换）、`shaders`、`compat`（旧规划入口）和 `modern`（`ScenePlan` 入口）。`lib.rs` 仅负责模块装配与公开导出；旧公开 API 暂时保持可用。
- 新增 `WgpuRenderer`，直接从 `ScenePlan` 构建目标顺序，不解析 `RenderPass`；内部持有主颜色 target、离屏/mask/destination 池，并提供由宿主提交的 `encode`、便利 `render` 和 `resize` 入口。输出可选择覆盖或预乘 alpha 叠加。主目标的 destination read 不要求宿主输出 texture 具备 `COPY_SRC`。
- 修复旧 shader 的 WGSL 多分量赋值、非遮罩扩展 shader 缺少函数定义及 Rust/WGSL uniform 对齐问题。这些问题在之前只有 CPU 规划测试时不会暴露。
- 网格和原始 mask 源复用相同的 GPU 顶点/索引缓存；视图矩阵放进 uniform。实际 GPU 用例在同一 renderer 的相机变化后检查模型顶点/索引上传字节为零。
- `sync_model`、`update_view` 和 `encode` 已拆开；`sync_model` 复制渲染数据而不持有调用者的帧或拓扑 `Arc`。非法模型或视图提交保留上一成功状态；encode 前检查纹理目录。模型网格、composite/present 几何、uniform buffer 和 bind group 均可复用；统计上传、创建、mask 重绘及附件字节数。
- 纹理目录支持内容版本。稳定版本的 mask 可跳过重绘，原地上传后递增版本会触发重绘；没有版本时保守重绘。
- `gpu_smoke.rs` 的 12 个真实 GPU 用例读回验证平面、不可见 raw mask 源、普通/反向及离屏 mask、普通/嵌套离屏、固定 Additive/Multiplicative、无宿主 `COPY_SRC` 的目标颜色读取、连续两次目标读取、纹理原地更新、错误后恢复与格式/尺寸切换。
- 独立宿主运行命令：`cargo run -p kasane-render-wgpu --example offscreen --locked -- target/wgpu-minimal.png`。它创建 device、源纹理和输出目标，提交绘制，读回并校验像素，再输出 PNG。
- 混合矩阵对照命令：`python3 tools/compare_wgpu_blends.py`（需要 Pillow、NumPy 和 Godot 可执行文件；可用 `--godot` 指定）。独立 WGPU 宿主对照现有 Godot shader 参考脚本，在 18 种颜色模式 × 5 种 alpha 模式 × 8 组 mesh/offscreen、遮罩及透明度样本中，720/720 格逐字节一致。可额外传入 `--official-probe target/wgpu-official-probe/kasane_framework_gpu_probe`，直接对照固定版本的官方 Framework GPU 探针：720/720 格通过，最大字节差为 1。探针可用 `cmake -S tools/probes -B target/wgpu-official-probe -DKASANE_CUBISM_ROOT="$PWD/third_party/CubismSdkForNative-5-r.5" -DKASANE_BUILD_GPU_PROBE=ON -DCMAKE_POLICY_VERSION_MINIMUM=3.5` 和 `cmake --build target/wgpu-official-probe --target kasane_framework_gpu_probe -j6` 构建。这个矩阵验证混合公式，不代替完整模型/应用端到端验收。

剩余后端工作：精确峰值显存统计、纹理和 GPU buffer 资源寿命的性能压测、官方 Framework 完整模型图像覆盖。正式应用还需新 WGPU 宿主的预览、选区覆盖层、截图与生命周期接入；旧公开兼容 API 待新入口稳定后移除。
