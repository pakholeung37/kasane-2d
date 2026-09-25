# Observe 视觉增强实施计划（全部 P0 / P1）

日期：2026-09-25。状态：待实施。范围已按用户要求纳入研究中的全部 P0、P1；本文 API、文件拆分及新增错误码是实施契约提案，不表示已经可用。

背景见 [研究文档](OBSERVE-VISUAL-ENHANCEMENTS.md)。本计划以当前 Rust/Python SDK 与 WGPU 渲染路径为入口，不依赖 GUI。聊天中的白兔演示仅展示交互效果，手工编号、截图放大不作为功能实现或验收依据。

## 1. 范围与交付标准

| 编号 | 优先级 | 必须交付 | 不可替代的验收点 |
| --- | --- | --- | --- |
| V01 | P0 | 明确 alpha/color 契约的展示图；浅色、深色、棋盘格、透明背景及 alpha 通道 | 半透明边缘正确；特殊混合不能被错误 unpremultiply；raw 兼容 |
| V02 | P0 | canvas ROI、mesh/Part 聚焦、padding、高清重渲染、固定/跟随视图 | 真实重新采样；图像↔画布映射；跨样本位置可比较 |
| V03 | P0 | 稀疏对象编号、引线、对象表、聚焦高亮 | 从图上标记能唯一查到当前工程对象；编号跨样本稳定 |
| V04 | P0 | 带参数标签的 contact sheet、参考/前后对照、onion skin、轮廓与差异热图 | 相同 view；目标/非目标区域分别统计；不隐藏位移 |
| V05 | P1 | 当前姿态 wireframe、顶点身份、位移箭头、变形器控制网格/轴、形变诊断 | 当前求值状态而非 base；压缩、拉伸、退化、翻折有数值证据 |
| V06 | P1 | context / isolated / xray；mask、合成层及遮挡排查视图 | 保留必要依赖；诊断不改 session；模式语义明确 |
| V07 | P1 | 点/区域查询、三角形/顶点对应、alpha/mask/offscreen 感知候选 | 不以包围盒充当命中；拾取身份不冒充最终颜色贡献 |

全部七项完成 Rust/Python 接口、独立 wheel 验证、文档及组合场景验收，才称 P0/P1 完成。单项遇到未支持组合时可以分阶段显式报错，但不能据此把整项标记完成。

不在本轮：P2 自动发现可疑参数区间、自动补采样、自动修形、审美评分、任意非线性 deformer 的全局逆解、所有像素的精确贡献分解、GUI 编辑器建设。指定参数样本与二维参数网格仍属于本轮 V04。

## 2. 已核对的代码约束

| 代码入口 | 当前事实 | 实施要求 |
| --- | --- | --- |
| `kasane-sdk-observe/src/lib.rs` | ObservationInput 保留 frame、资产描述、root；之后才读纹理 | 新捕获必须把同一批纹理 bytes 固定下来，供所有通道复用 |
| `kasane-sdk-observe/src/gpu.rs` | 每次 observe 固定配置、自动居中、读回 RGBA；PNG 原样编码 | 引入每视图 RenderRequest；旧入口语义不变 |
| `kasane-python/src/observe.rs` | session 锁内 capture，锁外 GPU；返回大 tuple | 新接口使用独立 typed result，避免扩充破坏旧 tuple |
| `python/kasane/_observe.py` | 标准库编码、每帧动态裁剪、未标注拼图、report v1 | 新 presentation/report 管线独立，旧 API 与 schema 保持 |
| `evaluation/types.rs` | Drawable 有 positions/UV/indices，没有 authoring vertex IDs 或求值后 deformer trace | 同一 capture 附加拓扑身份和可选求值 trace |
| `evaluation/transforms.rs` | 求值中的 TransformState 包含 pose、points、继承状态，未公开 | 诊断从此处导出受控 trace，不能 Python 重写求值算法 |
| `kasane-render/src/scene.rs` | 已有 mask、target、destination-read 和层内顺序 | 诊断依赖图与绘制顺序复用 ScenePlan |
| `render-wgpu/src/modern.rs`、`encoding.rs` | 有 resize；主场景/离屏/遮罩存在透明 clear；最终输出 Replace/Composite | 显式背景必须初始化实际主场景 target，不能只改最终输出 load |
| `kasane-sdk/src/diagnostics.rs` | 几何诊断看 base_positions | 保留旧行为，新增 evaluated geometry 诊断 |
| Python wheel | 目前无 Pillow runtime dependency，GPU 是可选 Cargo feature | 展示依赖隔离为 Python extra；CPU authoring import 不被阻断 |

当前工作树还有动画、CDI、Expression 等修改。实现每阶段前重新核对相关文件，不覆盖既有工作；新诊断能力不要求改变工程保存格式。

## 3. 架构与不可变捕获

数据流：`session / evaluated snapshot → captured scene → resolved textures → render views → presentation / diagnostics → packet → report`。

新增 `CapturedScene` / `ResolvedObservation`（名称可在首阶段统一）：

1. 同一次文档快照捕获 version、document ID、参数解析结果、对象名称/层级、顶点 ID、拓扑、几何空间、资产描述。名称解析不能跨快照查询；名称有歧义时报错。
2. 在冻结的文档上求值当前样本、基准与可选 deformer trace。多样本 run 必须使用同一文档快照，不在每帧读取正在编辑的 session。优先新增 SDK 只读捕获入口，避免长时间持有 Python session 锁进行 GPU/IO；若复制文档，记录复制成本。
3. 所需资产取所有样本和 mask 依赖的并集，每份资源只读取/解码一次，保留内容哈希和 RGBA。描述内有预期哈希时校验；不把多文件读取宣称为文件系统原子快照。后续修改源文件不影响该 capture。
4. 观察不修改 preview values、document、history、events。观察缓存是独立状态；捕获后的重命名不能改变旧 packet 的对象表。
5. 构造内部“从已求值 frame 捕获”的入口，为动画计划接入预留位置；不可再次按参数求值覆盖动画结果。没有完整匹配 metadata/trace 的外部 frame 不得伪造诊断能力。

身份分开：`capture_id` 是一次捕获标识；`scene_digest` 表示规范化场景/纹理内容；`render_digest` 再加入 view、模式、背景、采样策略；`artifact_sha256` 是实际文件哈希。旧 `input_sha256` 不改算法，新 digest 不使用 Debug 字符串作为长期协议。

canonical digest 固定字段顺序、数组/对象排序规则和浮点编码，拒绝 NaN/Inf，统一负零。名称变更影响对象元数据身份，不必强迫像素缓存失效。跨 GPU 不承诺字节相同，adapter/backend、SDK 与诊断版本进入报告。

## 4. 公共 API 提案

保持 `Observer.observe()`、`observe_run()`、`ObservedFrame` 与 report v1 兼容。新增接口全部是只读操作：

| 接口 | 职责 |
| --- | --- |
| `observer.inspect(session, values=None, *, request, baseline_values=None)` | 单个冻结场景生成 packet；baseline 与当前来自同一文档快照 |
| `observer.inspect_run(session, samples, *, request, output, baseline_index=None, layout=None)` | 冻结多样本、统一 ROI/编号、写 report v2 和分页图 |
| `observer.query(packet, *, view_id, point=None, region=None, mode="coverage", alpha_threshold=1/255)` | 对 packet 原场景查询，point/region 恰好一个；不读取 live session |
| `compare_observations(current, reference, *, view_id, reference_view_id, options)` | 包间对比与元数据检查；外部图片使用显式 ReferenceImage 适配 |
| `packet.save(absolute_directory)` | 将已有 packet 输出到唯一子目录；不重新求值/读资源 |
| `packet.close()` / context manager | 释放捕获资源；已保存报告仍可读取，后续 GPU 查询报 closed |

新增类型采用 frozen dataclass/明确字段的 Rust struct，不沿用需要按位置拆解的大 tuple。最小请求如下；示例为待实现 API：

```python
request = InspectionRequest(
    focus=Focus(mesh_ids=[mouth_id], part_ids=[]),
    view=ViewSpec(roi=None, resolution=(1024, 1024),
                  padding_canvas=12, framing="fixed_union", aspect="contain"),
    presentation=PresentationSpec(background="light", alpha="opaque"),
    channels=("clean", "labels", "wireframe", "deformers"),
    mode="context",
    limits=InspectionLimits(),
)
packet = observer.inspect(session, {"ParamAngleY": 30},
                          request=request, baseline_values={"ParamAngleY": 0})
hit = observer.query(packet, view_id="focus-0", point=(512.5, 512.5),
                     mode="coverage")
saved = packet.save(output_directory)
```

Request 分组：`Focus`、`ViewSpec`、`PresentationSpec`、`OverlaySpec`、`DiagnosticSpec`、`InspectionLimits`。通道包括 clean、labels、alpha、wireframe、vertices、displacement、deformers、mask；comparison 另由 compare/run 的显式基准触发。无基准时，不暗用“默认参数”计算伸缩或位移。

part 聚焦沿 Part 子树解析 mesh；deformer 层级单独记录，不能把 Part 当 deformer。重复对象去重；非法或不存在的显式 ID 报错；合法但无几何/出画面返回明确 empty 状态。全空自动 ROI 报 `EMPTY_FOCUS`；显式 ROI 允许空白结果。

默认 `inspect` 只返回 clean、labels；提供一张总览及至多两张请求的局部图。默认阈值、图像数量、选择省略规则在 report 中可见。请求超预算不静默降分辨率。

## 5. V01：展示像素契约

首阶段锁定以下策略并用小型 fixture 验证：

- raw 保持现有 RGBA8Unorm、混合与 PNG 字节路径。新展示默认用明确的、不透明浅灰背景；深色与棋盘格可选。报告保存实际背景色、棋盘格尺寸/原点，不能只保存主题名。
- 不透明背景进入 main scene target，先于任何颜色绘制和 destination-read；所有 mask 与子 offscreen 仍按其原本规则透明初始化。棋盘格作为背景绘制进入同一主 target。最终输出 Composite 不能作为此能力的替代。
- 透明 PNG 只在可表达的合成模式下输出 straight alpha。记录 unpremultiply 舍入与 alpha=0 策略；包含加法等不可由普通透明 PNG 忠实表达的组合时，显式报 `UNREPRESENTABLE_TRANSPARENT_OUTPUT`，提示使用不透明背景。该限制属于格式能力，不是静默丢失 RGB 的理由。
- 独立 alpha 图明确取自透明 raw pass，不取不透明展示图的全 1 alpha；标注为合成 alpha，不能用于证明特殊混合的视觉贡献。
- 首版保持当前数值颜色管线，policy 命名 `renderer_native_v1`，不自动增加 gamma。记录纹理采样、target format 与显示转换，不把 UNORM 自动解释成端到端线性色彩。若要新增 sRGB/linear 转换，另立 policy 与新基准。

门槛：普通半透明边缘、additive、multiply、extended/destination-read、mask、嵌套 offscreen 的浅/深背景结果有数值与图像验证；旧 raw 回归不变。透明表示失败是可预期的 typed error。

## 6. V02：相机、ROI 与坐标

统一输入空间为 source canvas pixels；输出图像坐标原点左上、y 向下，像素中心 `(i+0.5,j+0.5)`。连续 ROI 使用 `(x0,y0,x1,y1)`；栅格 bounds 最大端 exclusive。拒绝非有限值、零/负尺寸、非法 resolution。

`aspect="contain"` 唯一首版缩放策略，保持比例，不拉伸。ROI padding 后宽高为 rw/rh、输出 W/H 时：`s=min(W/rw,H/rh)`；`offset=((W-s*rw)/2-s*x0, (H-s*rh)/2-s*y0)`。返回 requested ROI、padded ROI、实际 view 的可见范围与 content rectangle；信箱边区查询返回 outside_content。ROI 可超画布，保留周边透明/背景，不自动夹紧而改变比例。

总览遵循原 Observer 设置；局部可用不同输出大小但不修改 Observer 默认配置。新渲染器请求以 resize/update_view 复用 GPU，失败后仍能继续正常观察。ROI 缩放与 mask_scale 同步；不裁掉画外 mask 依赖。

`fixed_union`：先捕获样本，按选中对象的求值几何范围并集求一次 ROI，再渲染每帧。基准若参与对照也进入并集。`follow`：每帧各自聚焦，明确不可用来判断绝对平移。用户显式 ROI 优先；不得在对比时自动重新居中。

每个 view 返回可逆 3×3 `canvas_to_image` / `image_to_canvas`，和 runtime↔canvas 变换。root 编辑可直接用 canvas；有父级时返回 geometry.space/parent。纯仿射父链提供逆变换；含 warp/glue 等路径只提供明确的渲染三角形对应，不宣称是 authoring keyform 的逆解。

V02 必须用亚像素位置、非正方形输出、非中心 origin、不同 PPU、越界 ROI 与旋转父级验证往返误差。高分辨率测试使用含细线的纹理，证明 rerender 与低分辨率放大不同。

## 7. V03：对象标记、表与高亮

一份 packet 始终保留无标注 clean。run 编号表从捕获到的对象 UUID 排序建立并固定，编号不随可见性变化；不保证另一个 run 编号相同。标签仅用数字，名称与完整身份放对象表，减少字体与遮挡负担。

对象表至少包含：mesh UUID、runtime ID、显示名、Part 路径、deformer parent、geometry space、enabled/visible/opacity、render order、composition target/path、mask IDs、拓扑 hash、geometry bounds。未知/未算 coverage 使用状态字段，不填零。

稀疏布局先标显式 focus，其次在 ROI 内按可配置次序选择；默认最多 12 个。标签尽量放几何轮廓外并用引线连接，冲突时省略低优先级标签，保存 `omitted_labels` 与原因。复杂图不强行填满编号；边缘候选不把锚点放到图外。

第一版几何高亮绘制所选轮廓/框且带 `geometry` 标识；V06/V07 接入后另有 coverage 轮廓。半透明的颜色覆盖只进入 annotated 图。名称重复、中文名称、隐藏对象与跨样本换层均应稳定映射。

## 8. V04：多状态与参考对比

`samples` 的输入顺序保留；每格有 frame index、参数 ID/显示名和 actual 值，clamp/repeat 导致与 requested 不同时明确标记。单参数按序列；二维布局显式声明 x/y 参数及采样值，不从任意多参数样本猜网格。缺格标 missing，重复格报错。

contact sheet 使用固定 view、尺寸、背景和 run 对象表；给每个格子记录 sheet 像素→view 像素变换，文本边栏不属于场景坐标。按图片像素预算分页，不把所有帧缩成不可读缩略图。

对比两类输入：

- packet 对 packet：校验 view、背景、color/alpha policy、尺寸、样本语义；跨工程需要显式 canvas registration，跨对象需要显式 object map。拓扑不同可以比较图像，不可直接比较顶点。
- 外部参考图：读取一次并记录哈希、alpha/color 解释、尺寸和显式 registration。缺少对齐元数据仍可并排展示，但数值结果标 `unregistered`，不输出伪精确几何误差。

生成并排图、onion skin、基于固定阈值提取的轮廓叠加、abs-diff heatmap 和变化 bounds。轮廓来源标明 alpha 或颜色边缘；内部五官不能用整体 alpha 轮廓代替。热图默认固定 0–255 范围，另可显式选择显示增益，报告保留范围与阈值。

指标固定为：不透明展示 RGB 的 MAE/max/超阈值像素比例、兼容 raw policy 下的 RGBA 差、透明 alpha 差。raw 不兼容时返回 unavailable。目标区采用显式 ROI 或记录下来的几何 union；非目标区为指定比较域的补集，不称精确语义分割。透明 RGB 与 alpha 分开，不能用大面积背景把局部错误稀释成一个好分数。

默认不做自动对齐、自动曝光/色彩归一化或自动阈值；这些会消除需要发现的误差。视觉相似不作为审美通过标准。

## 9. V05：求值几何与变形器诊断

新增可选 `EvaluationTrace`，由 Rust 求值过程产生；普通 evaluate 不付出保存 trace 的成本。保存各 drawable 的 evaluated positions 与 authoring vertex ID 对应、各 transform 的求值后控制点/旋转轴、父链以及使用的坐标空间。父级 warp 下轴线/网格需经过实际父链采样，不能将其误画为全局直线。

trace 必须覆盖当前支持的 rotation、warp、BlendShape 与 glue 结果。顶点 ID 从捕获的源拓扑对齐，验证 indices/顶点数量；triangle 身份用有序 vertex ID 三元组与 topology hash，不假装存在已有全局 triangle UUID。

显示通道：wireframe；稀疏 vertex ID；baseline→current 位移箭头；warp 控制网格与控制点索引；rotation 原点/轴；异常三角形覆盖。控制点索引只在该 deformer 拓扑版本内稳定，不能称稳定 authoring vertex ID。密度预算与省略信息进入 report。

数值统一在同一 canvas 空间计算，排除相机缩放：基准三角形 B、当前 C 的两条边构成矩阵，`F=C*inverse(B)`。用 `det(F)` 与两个 singular values 计算有向面积比、最小/最大伸缩，刚体旋转应为 1/1。分别报告 current 退化、baseline 退化、方向反转与阈值触发的局部压缩/拉伸。

默认提示阈值先采用最小伸缩 <0.5、最大伸缩 >2.0，全部可配置并显示；这是排查阈值，不是美术合格线。基准退化 epsilon 使用带单位且按尺度定义的策略，在首个算法 PR 固定；不得对不可逆 B 继续输出有限分数。

不同 topology hash、缺失 vertex ID、跨工程无映射返回 `TOPOLOGY_MISMATCH`/not_comparable。整体反射单独标注；若只有数值迹象而无可确认的反射来源，报告 `orientation_reversal`，不擅自归类为错误。包含 glue/混合后的位移不可直接当成某个 keyform 的修改量。

## 10. V06：隔离、X-ray 与遮罩视图

| 模式 | 精确定义 | 与原画面的关系 |
| --- | --- | --- |
| context | 完整原场景，仅在派生图加标记/高亮 | clean 是真实场景结果 |
| isolated | 保留目标颜色绘制、其祖先合成层和全部 mask 依赖，省略其他颜色绘制 | 是明确背景上的隔离场景；不承诺与原场景对应像素相同 |
| xray | 默认按 geometry 展示选中对象，越过遮挡；可显式选择忽略 mask/opacity/disabled | 属诊断重写，图内标记 XRAY，报告逐项 overrides |

在 ScenePlan 旁构造不可变 `DiagnosticPlan`，分开记录 color items 与 mask-only dependencies。遮罩源即便被省略为颜色绘制仍要参与 mask。保留嵌套 target 路径、offscreen mask/opacity、反转 mask 和各 target 内顺序；不得把层内 render_order 简单全局排序。

isolated 中 destination-read 读取隔离场景在该时刻的背景/下层结果，复用正常混合实现；report 标注 `destination_context="isolated"`。不得声称完全保持原始混合背景。若选中 mask source 后也省略其消费者，这一模式不用于因果判断。

mask 视图导出原始 mask source 几何/alpha、组合 mask、反转后的 consumer mask 与 consumer 覆盖；标明 mask target、分辨率、采样变换、source IDs。保留 CPU/GPU 缓存身份，避免开诊断后影响普通渲染结果。

X-ray 若要观察被 disabled 的对象，需显式捕获 hidden geometry；不可把缺失求值 positions 当成空网格后标成功。overrides 不回写源 frame，所有诊断使用派生 plan/独立副本。

遮挡排查提供 context + isolated + mask + compositing path；精确的“移除目标前后差分”可作为可选诊断后续增加，不作为本轮必备，更不包装为精确贡献率。

## 11. V07：点/区域查询

查询必须指定 packet/view，坐标采用该 view 的像素；标注图使用与 clean 相同 scene transform。初版不从截图颜色反推 ID。

| 查询层级 | 本轮交付语义 | 必须避免的误解 |
| --- | --- | --- |
| geometry | AABB 粗筛后实际三角形相交；点返回所有命中三角形及重心坐标，区域返回真实三角形相交 | AABB 相交不是 mesh 命中 |
| coverage | 对候选求纹理 alpha × opacity × mesh mask，并应用祖先合成层的有效 mask/opacity gate；返回采样依据 | 这是定义好的覆盖，不是最终像素颜色占比 |
| frontmost_covered | 依 ScenePlan 完整合成顺序选最前的覆盖候选，返回候选列表与 target 路径 | 半透明/特殊混合下只是选择规则，不是唯一视觉贡献者 |

coverage 首版采用按候选的 GPU 诊断 pass，复用实际纹理过滤、UV、double-sided/culling、mask 参数与祖先路径；小 ROI/点查询先粗筛再读回，不为每个 mesh 固定分配全幅图。不能从最终合成 alpha 反推出逐对象 coverage。

当 blend/alpha 运算无法用上述 gate 表达时，返回 `coverage_status="unsupported_composition"` 与明确的几何候选，禁止自动当作 coverage 成功。实施 O6 必须建立 renderer 支持模式矩阵：所有正常、additive、multiply 的本地覆盖可查询；扩展 alpha/composite 模式逐项选择真实诊断传播或声明能力限制，不能全局宣称精确可见像素。几何查询、isolated/context 图仍必须可用。

coverage 命中定义为采样值 >= 阈值，默认 1/255。点坐标落入像素 cell 时读对应像素中心并同时返回 requested/sample 坐标；geometry 模式则可用连续点。出图/信箱区不夹到边缘，返回 outside。区域按半开整数像素域统计满足阈值的像素数和 bounds；完整返回数受 max_hits 限制时附 total/truncated。

结果字段：mesh ID、mark、composition path/order、triangle key、vertex IDs、barycentric weights、image/canvas/runtime 位置、UV、coverage value/status、masks、geometry space/parent、命中原因。UV 插值与 PNG 行方向沿用项目 UV convention。

纯仿射父链可提供 `editable_point` 和矩阵；warp/glue 路径仅提供渲染表面对应及明确的 `edit_mapping_status`。重叠/翻折命中多个三角形全部可返回，不任意选择一个伪装成唯一逆解。

## 12. Packet、报告与资源管理

report v2 独立于现有 observe_run v1。结构至少包含：

```text
schema_version, status, run_id, sdk/build/platform/adapter
capture: id, document/version/evaluation/source revisions, digests, textures
samples: requested, actual, frame/capture IDs
objects: mark → UUID, names, parents, topology, drawing metadata
views: id, kind, sample, mode, path/hash, dimensions, requested/actual ROI,
       matrices, pixel/color/alpha/background policy, dependencies/overrides
comparisons: input view IDs/hashes, registration, masks/ROI, thresholds, metrics
diagnostics: code, severity, object/triangle IDs, values/units, evidence view IDs
capabilities: per-channel/query status and limitations
resources: timing, render/readback counts, output pixels/bytes, peak estimates
```

落盘路径使用内部 view/sample ID，不直接拼接对象名称；文件相对 run 目录，返回 receipt 包含实际绝对路径。JSON 禁止 NaN/Inf，不可计算用 null + reason。成功完成所有请求通道才 `complete`；调用方显式允许的不可用通道记 `partial`，GPU/IO 失败记 `failed` 并附已完成项目。默认不吞掉请求通道的错误。

先写临时文件再 rename 完成单个 artifact；每步更新可读 report，失败保留证据且异常带 run_directory。不得覆盖别人的目录。V2 reader 拒绝未知主版本；新增可选字段向后兼容。保存包可离线查看/比较已有图；没有纹理/scene payload 的包不支持重新 GPU 查询，应返回 `CAPTURE_NOT_AVAILABLE`。

默认限额作为初始实现配置：每 view ≤4096×4096 且受设备限制；每页 ≤16M pixels；总 artifacts 像素 ≤64M；samples ≤64；labels ≤12；顶点标签 ≤64；query candidates ≤256。像素预算计算含派生图，限制可显式调整；超限提前报 `OBSERVATION_BUDGET_EXCEEDED` 并给请求与限额。

另设 CPU retained bytes 默认 256 MiB，记录纹理解码、frame/trace 与 image buffers，超限不启动 GPU；大 run 顺序渲染/落盘、分页，不保留所有 RGBA。现有 renderer offscreen/mask 预算继续生效且单独报告；这两个限额不是总进程内存保证。查询缓存按 capture/view/policy/drawable/threshold 键与容量淘汰，close 释放 capture 所持资源。

## 13. 模块与依赖落点

| 模块 | 拟改动/新增文件 | 负责内容 |
| --- | --- | --- |
| kasane-core | `evaluation/types.rs`、`evaluator.rs`、`transforms.rs`、`meshes.rs`、`glue.rs` | 可选 trace、隐藏几何与当前姿态对应；不画文字/PNG |
| kasane-sdk | 新 `observation_snapshot.rs`、诊断纯计算模块 | 冻结 authoring metadata、vertex IDs、父链与 evaluated geometry 诊断 |
| kasane-sdk-observe | 从 `gpu.rs` 拆 `capture.rs`、`view.rs`、`inspection.rs`、`query.rs` | 纹理生命周期、view、渲染请求、packet 原生数据、GPU query |
| kasane-render | 新 `diagnostic.rs`；复用 `scene.rs` | 模式与依赖闭包、合成路径、diagnostic plan |
| kasane-render-wgpu | `modern.rs`、`encoding.rs`、`shaders.rs`；新诊断 pass 模块 | 主 target 背景、coverage/mask 输出、buffer readback |
| kasane-python | 新 native inspection bindings、`_inspection.py`、`_presentation.py`、`_comparison.py`、新 types | 公共请求/结果、布局/标注、对比、schema、保存 |
| tools/tests/docs | 新 `tests/fixtures/observe_inspection/`；扩展 wheel validator 和 API/README/VALIDATION | synthetic fixtures、集成门禁、可运行 recipe |

Python 新增 `inspection` extra，首选 Pillow 负责字体、标注、布局和图片处理；版本需在 CPython 3.14 安装验证后固定支持范围，开发依赖更新 root pyproject/uv.lock。基础 `import kasane` 与旧 observe 不要求该 extra；调用展示功能缺依赖给可操作错误。数值 geometry/SVD 留 Rust，避免再加 NumPy。

内置确定字体资源与许可记录，默认编号使用小型数字/ASCII 字体；中文显示名原样保留 JSON。图内完整名称允许显式字体，缺字时回退到编号与 runtime ID 并报告，禁止依赖实现机器的系统字体导致标签位置漂移。

## 14. 实施阶段与 PR 顺序

阶段编号 O0–O7 与功能优先级 P0/P1 不同。每阶段交付可运行入口与证据，不只增加空类型。

| 阶段 | 依赖 | 工作包 | 阶段验收 |
| --- | --- | --- | --- |
| O0 契约与基线 | 无 | 固定请求/schema、synthetic fixture、raw 基线、背景及混合 probe、内存/耗时基线 | 现有结果可复现；展示/透明支持矩阵与公式明确；不改旧行为 |
| O1 捕获与 ROI | O0 | immutable capture、纹理一次解析、对象/拓扑元数据、view matrices、ROI rerender、基础 packet | 多 view 与 run 同一快照；新图细节/坐标验证；旧 observe 不回归 |
| O2 展示与定位 | O1 | main background、透明/alpha policy、presentation extra、编号/高亮、对象表 | V01/V02/V03 的正常与异常路径通过独立 wheel |
| O3 对比与报告 | O2 | 固定样本 ROI、二维布局、分页、compare、热图/轮廓、v2 完整发布 | V04 全部通过；不兼容参考与失败 run 不假成功 |
| O4 形变诊断 | O1、O2 | evaluated trace、wireframe/vertex/deformer、位移、面积/SVD 诊断 | V05 覆盖 rotation/warp/BlendShape/glue；camera 不影响指标 |
| O5 隔离与 mask | O1、O2 | DiagnosticPlan、closure、isolated/xray、mask 图、特殊混合验证 | V06 各模式、嵌套/反转/画外 mask 通过；诊断前后 clean 相同 |
| O6 查询 | O4、O5 | geometry point/region、coverage pass、composite-aware ordering、映射与 limits | V07 命中、透明/遮挡、拓扑歧义、能力限制如实返回 |
| O7 联合验收 | O3–O6 | 组合回归、wheel recipe、性能/资源回收、agent 消融、正式文档 | 七项 traceability 全通过；未运行项与能力限制公开 |

建议每阶段分 1–3 个 PR，按“契约/算法 → 原生入口 → Python 与端到端”拆分；有依赖的步骤顺序执行。O4/O5 在 O2 稳定后相互独立，但默认不启动并行 agent。实施从 O0 开始，不先改旧 observe 的返回类型。

阶段台账每项保存：源码 revision/工作树摘要、fixture hash、命令、产物路径、通过/失败/not_run、已知限制。完成 O2 仅代表第一批 P0；完成 O3 才能标全部 P0；O4–O6 全部通过才是功能层面的 P1。

## 15. 验证矩阵与发布门禁

| 测试组 | 关键正/负控制 | 对应功能 |
| --- | --- | --- |
| capture | capture 后改名/改纹理/编辑顶点；run 中 session 更新；旧 packet 不变 | 全部 |
| pixels | 半透明边缘、RGB≠0 且 alpha=0、浅/深/棋盘背景、extended blend、nested offscreen | V01/V06 |
| views | 非中心 origin/PPU、不同 aspect、亚像素、越界、画外 mask、固定 union vs follow | V02 |
| marks | 重名、中文、隐藏/换层、12+密集部件、跨帧稳定、省略记录 | V03 |
| compare | 单/双参数、缺格/重复格、不同 gamma/alpha/view、未注册参考、分页坐标 | V04 |
| geometry | 刚体旋转 1/1、已知比例压缩、局部翻折、全局反射、退化基准、remesh、warp 父链与 glue | V05 |
| composition | mask-only source、反转、nested targets、destination-read、disabled xray、缓存恢复 | V06 |
| picking | 透明 texel、三角形外但 bbox 内、重叠与半透明、不同 target order、边界、mask gate、多个三角形 | V07 |
| integration | 旧 observe tuple/v1、CPU wheel 无 extra、GPU wheel+extra、仓库外执行、失败后 observer 恢复 | 全部 |
| resources | 超限预检、顺序批处理、partial/failed 输出、重复 close、重复 inspect/query 内存稳定 | 全部 |

纯算法在 CPU 测：坐标往返、triangle intersection/barycentric、SVD、closure、schema 与布局。GPU 测实际纹理/mask/混合/覆盖，不用单纯快照测试替代。合成 fixture 用解析预期/独立参考；跨 GPU 容差预先记录，不放宽容差掩盖语义错误。

renderer 改动必须跑现有 blend/mask/offscreen 回归；不修改旧图基线来迁就新展示路径。新增展示基线单独记录 policy。官方 Framework 可用时，复用现有固定参考验证正常渲染；诊断专有通道用可计算 synthetic 真值，不能声称官方有同名接口。

实施阶段使用的既有命令（本计划编写时未执行运行时测试）：

```sh
cargo test -p kasane-sdk-observe --locked
cargo test -p kasane-render --locked
cargo test -p kasane-render-wgpu --locked
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
uv run --locked maturin build --manifest-path modules/kasane-python/Cargo.toml --release --features observe --out target/python-wheels
```

wheel validator 需增加可选 inspection suite，并在新功能发布 profile 中强制执行且安装 extra；旧 `--full` 的检查不能被替换掉。实际 wheel 路径与 probe 参数遵循 [VALIDATION](VALIDATION.md)，不在计划中假定一次构建后的文件名。

O7 性能报告分别记录 capture、decode/upload、每通道 render/readback、presentation、保存、峰值内存；对比同 resolution 的旧 observe 与 clean-only inspect。O0 确定设备/fixture/重复次数后再设耗时阈值，不在没有测量时承诺毫秒数。资源上限、无泄漏与不重复读纹理是确定门槛。

Agent 验收使用 S4-A、S4-B v2、白兔局部压缩/换层遮挡及一份未调优的新素材。冻结模型/SDK/输入/预算，独立会话，平衡顺序。分组为旧 observe、+ROI、+编号/对比、完整 P0/P1，分别测对象定位、坐标误差、非目标修改、任务完成/重放、调用次数/耗时/图像预算。小样本只报告原始结果，不能将 UI 看起来清楚当作有效性证据。

## 16. 首个实施批次

O0 开始时先完成三项具体产物：

1. `docs/OBSERVE-VISUAL-CONTRACT.md`：最终 typed request/result、报告 schema、坐标/像素/查询语义、逐混合模式能力矩阵；与本计划有差异时明确记录。
2. `tests/fixtures/observe_inspection/`：自建透明 padding、细线、两层遮挡、旋转父级、warp 压缩、mask/offscreen 的最小工程与输入哈希；不复制受限制角色素材。
3. `docs/OBSERVE-VISUAL-BASELINE.md`：现有 raw、背景 probe、耗时/内存与可执行命令，失败与未运行如实记录。

随后进入 O1。整体范围不因阶段划分而缩减；如新发现要求改变坐标或像素契约，先更新契约和对应负控制，再改实现。本轮计划文档本身不表示 O0 已完成。
