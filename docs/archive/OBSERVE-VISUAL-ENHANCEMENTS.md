# 面向 Agent 的 Observe 视觉增强研究

日期：2026-09-25。状态：设计建议，尚未实现或进行增强功能的 agent 对照实验。

后续范围已确定为全部 P0、P1，具体依赖、接口契约和验收见 [实施计划](OBSERVE-VISUAL-IMPLEMENTATION-PLAN.md)。研究中的阶段建议由实施计划细化；P1 点/区域查询包含 alpha/mask/offscreen 感知能力，P2 自动补采样仍后置。

## 结论

建议围绕四个问题建设：看清局部、将画面定位到可编辑对象、比较不同状态、解释形变与遮挡。第一批实现显示规范、固定区域高清观察、稀疏对象编号和多状态对比；随后加入网格诊断、隔离视图与遮挡查询。

每次观察应提供相互关联的 clean image、诊断图和结构化数据。原图用于判断视觉效果，诊断图解释对象和几何，JSON 提供精确坐标、身份和版本。标注不应覆盖唯一的原图；默认不要一次返回所有诊断通道。

## 当前实现与实际使用证据

| 已有能力 | 已核对的边界 | 对设计的影响 |
| --- | --- | --- |
| `Observer.observe` 返回 RGBA、PNG、参数、画布、view、版本和纹理哈希 | 视图仅公开输出尺寸与 `fit_long_side`；未提供指定 ROI 的平移视图 | 高清局部需要明确相机区域，不能仅放大旧截图 |
| `drawable_bounds` | `visible` 只检查 enabled、visible、opacity；bounds 是顶点投影包围盒，加 2px 后裁到画面 | 不能称为可见像素区域，也不能用它证明没有被遮挡 |
| `observe_run(focus=...)` | 在最终合成 RGBA 中按每帧部件 bounds 裁剪；保留其他对象和遮罩效果 | 这是上下文裁剪，不是隔离；逐帧自动框选会掩盖整体位移 |
| contact sheet | 按样本顺序拼成近似正方形，没有图内参数标签 | 需要参数轴、帧号、固定视角与分页 |
| `evaluate_snapshot` | 提供完整求值网格及绘制属性；`evaluate` 仅提供精简 positions | 可复用快照做网格与对象标注，但必须与观察图版本一致 |
| `diagnose_geometry` | 当前检查基础几何的小三角形、绕序与画布范围 | 不能替代指定参数状态的形变检查 |

实现依据：[GPU observer](../modules/kasane-sdk-observe/src/gpu.rs)、[Python observation](../modules/kasane-python/python/kasane/_observe.py)、[Python API](../modules/kasane-python/API.md)、[geometry diagnostics](../modules/kasane-sdk/src/diagnostics.rs)。

已有实验支持以下需求，但尚不能证明增强功能的收益：

- [S4 实验](experiments/ROUND-2026-09-24.md)已验证从参考图定位与旋转父级修正可完成；新增功能应测量是否减少定位、坐标转换和修复成本，不能预设现有流程失败。
- [白兔 X 实验](experiments/SHIROUSAGI-HEAD-X.md)记录大量透明边缘顶点落在参考 mesh 外，说明几何范围与有效图像区域需要区分。
- [白兔 XY 实验](experiments/SHIROUSAGI-HEAD-XY.md)记录分组后脸遮住五官、共享 warp 将局部高度压到 43.3%；后续按用户视觉反馈修复，允许参考像素误差上升。结构合法、像素接近与形态合适是不同结论。

## 建议功能与优先级

| 优先级 | 功能 | Agent 得到的信息 | 实现范围 |
| --- | --- | --- | --- |
| P0 | 显示背景与 alpha 规范 | 稳定的浅灰背景预览，可选深色/棋盘格、独立 alpha 图 | Python 派生图为主；特殊混合背景语义需 renderer 验证 |
| P0 | 固定 ROI 与高清重渲染 | 总览 + 有位置框的局部高清图 + 双向坐标映射 | Rust observer 增加 ROI view，沿用 renderer viewport |
| P0 | 稀疏编号与对象表 | 图上短编号 → mesh UUID、名称、父级、Part | 一致的 observation/evaluation 快照 + Python 标注 |
| P0 | 参数对照、参考对照和差异图 | 相同区域的 before/after、轮廓叠加、局部误差 | Python；新增报告 schema |
| P1 | 网格、顶点与变形器诊断 | 顶点身份、位移箭头、压缩/拉伸和翻折位置 | 求值数据、拓扑对应、诊断 overlay |
| P1 | 高亮、隔离、遮罩诊断 | 目标长什么样、使用哪些 mask、在哪个合成层 | render plan 与 WGPU 增加诊断路径 |
| P1 | 点/区域查询 | 指定位置有哪些候选对象及命中依据 | 先几何候选，再按需增加 alpha/mask 感知查询 |
| P2 | 自动发现可疑参数区间 | 对消失、突变、异常压缩自动补采样 | 建立 P0/P1 后再扩展参数扫描 |

### 1. 显示图必须有明确的像素语义

当前 `png_bytes` 将 GPU RGBA 原样编码，report 明示 `premultiplied_no_post_conversion` 与 `linear_unorm_no_gamma_conversion`。PNG 标准要求非预乘 alpha，因此普通查看器对半透明像素可能再次乘 alpha，产生暗边。这是代码与格式契约的差异；本轮未实测具体查看器的显示程度。[PNG alpha 规范](https://www.w3.org/TR/png/#6AlphaRepresentation)

建议保留兼容的 raw buffer 和现有基线，新增有版本的 presentation 输出：普通 source-over 可合成到已知背景形成不透明 PNG；透明展示图需明确转换为 straight alpha。不能仅凭 `linear_unorm` 元数据盲目增加 gamma 转换，应核对纹理解释、混合和目标显示的完整约定。

加法等混合可能出现 RGB 非零而 alpha 为零的输出，通用 unpremultiply 无法保存其显示语义；对这类模型优先使用显式背景的渲染结果，记录背景是在渲染阶段还是事后合成。显示转换与像素回归应分别测量。

### 2. 局部观察应保留位置和真实细节

支持按 source-canvas ROI、对象集合、Part 指定观察区域，提供 padding 与输出像素尺寸。多状态比较默认使用所有样本几何范围的并集或用户固定 ROI，所有格子采用同一变换；另提供显式的逐帧跟随模式。

区别记录 `crop`、`resized_crop`、`rerender`。旧图放大不能新增采样细节；高清 rerender 通过新的 scale/offset 和 mask_scale 重新渲染，保留视野外但仍参与遮罩或合成的依赖。

每张图返回 `image_to_canvas`、`canvas_to_image`、实际 ROI、像素尺寸，约定左上原点、y 向下、像素中心为 `(i+0.5,j+0.5)`。当前 view 下画布投影为 `image = scale * canvas + offset`；运行时坐标需先经画布 origin、pixels_per_unit 与 y 翻转。

画布坐标不是所有 mesh 的可编辑坐标。旋转父级可提供逆变换；warp、折叠与多层变形不能承诺唯一全局逆解。后续可通过 triangle ID + barycentric weights 建立局部对应，并明确返回歧义或不支持。

### 3. 对象标记解决“看到了，但不知道改谁”

一张 clean 图配一张 annotated 图。只标选中区域或指定对象集合，默认限制标记数量，其他对象在 JSON 中标明被省略；使用引线将文字放到空白处。一个 observation run 内按 UUID 固定短编号，跨样本不重新排序。编号与名称分开，名称不作为唯一身份。

第一版使用几何 bounds/轮廓并注明 `geometry`，不假装知道最终可见轮廓。对象表包含 UUID、runtime ID、显示名、Part、deformer parent、坐标空间、render order 与 mask IDs。不同工程间对照必须由调用方提供对象映射，不能认为编号相同就是同一部件。

[Set-of-Mark 研究](https://arxiv.org/abs/2310.11441)提供了“可引用视觉标记帮助 grounding”的实验依据。Kasane 已有对象身份，可以直接从场景生成标记，无需先做图像分割；但研究模型与任务不同，收益仍须本项目对照实验确认。

### 4. 对比应定位变化，而非只给一个分数

提供带参数标签的 contact sheet：单参数按序列，双参数按 X×Y 网格，超出图片尺寸/数量预算时分页。同样本的参考与结果共用 ROI、缩放和背景。

差异包包含并排图、可选 onion skin/双色轮廓、绝对差热图、变化区域 bounds、目标区与非目标区的误差。热图附固定或明确记录的数值范围，避免每张自动归一化使噪声看起来像大错误。透明区的 RGB、alpha 和展示背景上的颜色差分分别定义。

不默认对图片自动对齐，因为对齐会隐藏本来要发现的位置错误。外部参考没有相机元数据时要求显式注册变换，或者标记为仅供视觉比较。坐标一致并不意味着跨 GPU 可逐像素相同。

[Cubism onion skin](https://docs.live2d.com/en/cubism-editor-manual/onion-skin/)已有关键形态前后状态、选中顶点轨迹等交互依据。面向 agent 可将这些信息导出为带标签的静态图与数值轨迹。

### 5. 网格诊断必须描述当前姿态

对选中 mesh 展示求值后的 wireframe，顶点编号使用稳定 vertex ID，不使用不稳定数组下标；保留基准状态、当前状态及二者对应。优先绘制异常三角形和抽样位移箭头，避免把几千个点全部标到一张图。

建议统计相对基准三角形的有向面积变化、局部最小/最大伸缩、退化与翻折。需要拓扑一致，基准退化时返回不可计算；刚体旋转不应被误报为压缩，整体镜像也需与局部翻折区分。告警是可解释的几何提示，不直接判定审美错误。

### 6. 隔离与遮挡不能用简单隐藏替代

明确三种模式：`context` 保留完整场景；`isolated` 展示目标及必要 mask/offscreen 依赖；`xray` 忽略部分遮挡规则，必须有诊断标识。先实现依赖闭包，再考虑支持隔离；不要临时编辑 session 的 visibility 来渲染诊断图。

复用 [ScenePlan](../modules/kasane-render/src/scene.rs) 的 target、mask、destination-read 关系。multiply、destination-reading 和 offscreen 使隔离结果依赖背景；无法保证原场景语义时显式标注受限模式。

几何命中只返回候选。覆盖查询必须考虑纹理 alpha、mask、反转 mask、绘制顺序和合成层。单一整数 ID buffer 只能表达定义好的拾取规则，不能完整表达多个半透明对象的贡献。

visibility 数据分层命名：`geometry_bounds`、`coverage_before_occlusion`、`pick_owner`，记录 alpha 阈值及能力范围。需要解释最终画面的因果影响时，可按需比较移除目标前后的重渲染差异，但这是额外渲染的差分证据，不等同于精确贡献率或可见面积。

## API 与实现边界

以下为概念接口，不是现有 API，也不是最终签名：

```python
packet = observer.inspect(
    session,
    values={"ParamAngleY": 30},
    focus=[face_mesh_id],
    roi=fixed_canvas_roi,
    resolution=(1024, 1024),
    channels=["clean", "labels", "wireframe"],
    baseline=neutral_packet,
)
```

保留轻量 `observe()`；新增 inspection packet 聚合派生视图。一次 capture 固定文档、求值数据、纹理内容与对象元数据，派生图关联同一个 capture ID。当前 ObservationInput 已保存求值帧，但资产是在之后 resolve；若宣称所有通道来自同一输入，必须复用已解析的纹理数据，不能为各通道重新读取可变文件。

report 建议扩展 schema v2：`capture_id`、已有版本/哈希、`views`、`objects`、`diagnostics`、`capabilities`。每个 view 标注 kind、路径、哈希、尺寸、坐标变换、样本、背景、alpha/color policy、标注选项及基准 capture。未计算或不支持的 coverage 返回状态，不能填 0。

Python 负责布局、标注、差异图和报告；Rust observer 负责一致捕获、ROI 与准确坐标；renderer 负责需要真实 mask/composition 语义的诊断 pass。暂不将所有图像包装需求塞进 WGPU。

默认输出量建议从“1 张总览 + 1 张标记图 + 最多 2 张局部图”试验起步，细密 wireframe 和全量对象表按需请求。图像预算、耗时和质量需要共同测量；这个数量是初始产品假设。

## 验证与分阶段交付

第一阶段：明确 presentation 像素契约，固定 ROI、高清 rerender、编号和坐标映射，带标签的对比图与 schema。第二阶段：当前姿态网格诊断、依赖安全的隔离和候选拾取。第三阶段：像素覆盖查询及自动补采样。

实现时应验证这些真实风险：

1. 半透明边缘在浅色/深色背景的合成；加法和乘法；显示转换不改变 raw 基线。
2. 不同尺寸、非中心 ROI、y 轴转换、像素边界和旋转父级；验证图上点能映射回正确空间。
3. 透明 padding、完全遮挡、出画布、反转 mask、嵌套 offscreen；不混淆 geometry 与 coverage。
4. 多帧固定 ROI 能保留真实位移；编号稳定；不同版本不混入一个 packet；观察不改变 session/history。
5. 端点正常但中间翻折、局部压缩、换层后五官被遮挡；记录发现率和误报率。

Agent 对照先使用已有 S4-A、S4-B v2，再加白兔多层重叠与未参与调优的新素材/层级变体。冻结 wheel、模型、输入、预算，采用独立会话并平衡任务顺序，避免沿用学习过答案的会话。比较原 observe、加高清 ROI、加编号、加差异/几何诊断的消融组。

分别记录正确对象定位率、坐标修正误差、非目标改动、重放结果、渲染/工具调用次数、总耗时与图像预算。若能够获取模型输入 token，再记录 token；不要用图片文件体积代替。完成率和代价分开报告，小样本不宣称统计稳定。

本轮仅完成源码、既有实验和外部一手资料研究；未修改运行时代码、未运行新 GPU 验证，也未证明上述增强会提升 agent 成功率。
