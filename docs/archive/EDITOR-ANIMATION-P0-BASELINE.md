# Framework 动画 P0 CPU/GPU 兼容基线

日期：2026-09-25。范围：独立参考探针、自建 fixture 和复现报告。

## 复现与来源

在仓库根目录运行：

```sh
uv run --locked python tools/run_framework_animation_cpu_probe.py
uv run --locked python tools/run_framework_opacity_gpu_probe.py
```

脚本每次重新生成自建 MOC3/physics3 fixture，重新配置并构建 CPU 探针，不按旧二进制存在与否跳过构建。结果写入 `target/animation-cpu-probe/results/`：`report.json` 是总状态，`reference.json`、`real_control.json`、`repeat.json`、`motion_loop.json`、`physics_fps.json`、`numeric.json` 是实际参考输出，旁边的 `.log` 保存命令、退出码、stdout、stderr。开始时先原子写入 `failed/incomplete`，超时或任何验收失败会覆盖旧状态。探针使用正常的 `CubismFramework::StartUp/Initialize/Dispose/CleanUp`；CPU 目标单独提供无图形资源的 `CubismRenderer::StaticRelease()` 空实现，GPU 目标不链接它。

本机参考使用 `third_party/CubismSdkForNative-5-r.5`，官方 Core 报版本 `6.0.1`、整数 `100663297`。本轮 Framework `src` 中 `.cpp/.hpp/.h` 的排序路径加文件 SHA-256 汇总哈希为 `5f0ef0e6ed8581368b7340fcac1d09f955f775932d6465ed088894f93e91f258`；Core `include` 对应哈希为 `66a0b225f6c603b0e6bce8c0994c79c797847c3bfc454abfe6b805c4b787844d`，Core 静态库 SHA-256 为 `318da4dcfb4ced7221f7ec1487541152dee8c5275059b74156768d66499f0238`。每次运行的报告还记录探针二进制、CMake、运行脚本和每份 fixture 的哈希。目录名不作为 SDK 来源证明。探针不提交 SDK 源码或厂商样例。自建 MOC3 从仓库已有 `tools/create_v50_external_fixture.py` 生成的 synthetic v5 fixture 派生；`tools/create_animation_cpu_moc3.py` 固定并校验该输入哈希，再构造两真实 Part、两 drawable 和两种同名控制参数情形。

当前总状态为 `partial`：以下 CPU 行为、负控制及独立 GPU opacity/Offscreen 像素对照通过；复杂场景和宿主最终合成尚未验收，不能称完整渲染一致。

## 参考行为

| 项目 | 本机参考结果 |
| --- | --- |
| PartOpacity → Pose，虚拟控制 | 真实参数为 `ParamX/ParamY`，真实部件为 `Part0/Part1`，drawable 父部件为 `[0,1]`。同名控制参数索引为虚拟槽 `[2,3]`。Motion 把控制值从 `[1,0]` 写为 `[0,1]` 后，Pose 在 `dt=0.25` 把 Part opacity 写成约 `[0.7,0.5]`，下帧为 `[0,1]`；相应 drawable opacity 在首帧约 `[0.6125,0.4375]`。 |
| 真实同名控制 | 第二个自建 MOC 把第二真实参数 ID 设为 `Part0`，`Part0` 控制索引为真实槽 `1`，`Part1` 控制索引仍为虚拟槽 `2`。Motion/Pose 后同样得到 `[0.7,0.5]` 和 `[0,1]` 的 Part opacity。 |
| repeat | MOC 中 `ParamY` repeat 位为 true，范围 `[-1,1]`。Framework 默认模型级 override=true 时有效 repeat=false，输入 `1.25` 读回 `1`，输入 `-1.25` 读回 `-1`。调用 `SetOverrideFlagForModelParameterRepeat(false)` 后有效 repeat=true，二者分别读回 `-0.75`、`0.75`。精确最大值 `1` 仍读回 `1`；在此 fixture 的 Core 几何中，最大值处 drawable 顶点 Y 为 `-1.5`，与最小值处相同。输入跨整周期的 `3` 读回 `-1`，`-3` 读回 `1`。这些是两种明确配置的参考，不改变 Kasane 现有默认。 |
| Model opacity | Motion `Model/Opacity` 曲线使 CPU Model opacity 依次为约 `0.3/0.4/0.5`。对应 drawable opacity 是 Part/Core 结果，例如第一动画帧为 `[0.6125,0.4375]`，未在 CPU drawable 值里再乘 `0.3`。GPU 像素对照确认仅调用 `model->SetModelOpacity(0.5)` 不改变 Framework renderer 输出；显式调用 `renderer->SetModelColor(...,0.5)` 才改变本探针渲染 alpha。宿主如何使用 Model opacity 需单独实现，不能在 Core drawable opacity 再乘一次。 |
| 静态 Offscreen + Part opacity | 官方 GPU probe 对一个 Offscreen/Part0 双层透明度场景分别渲染。Offscreen 0.5 与 Part0 0.5 各自的整图相同；两者合用时左侧样本 alpha 从 `112` 变为 `56`，另一 Part 像素不变。 |
| Motion V2 | 新建 Motion 默认行为枚举为 `1`（V2）。fixture 的 `Duration=1`、`Fps=2`，探针显式调用 `SetLoop(true)`。在时间 `1.25/1.5/1.75/2.25`，V2 的 `ParamX` 为 `1/0.5/1/0.5`；显式 V1 对照为 `1/0.25/0.5/1`。V2 的延长一帧与跨边界余量保留都在真实队列更新中可观察。 |
| Physics Fps | 同一自建单 rig、相同 `dt=[1/60,1/60,1/60,1/60,1/30,0.1]` 与输入序列 `[0,1,1,-1,-1,0]`。缺 Fps 与 Fps=0 的六帧输出完全相同，第三帧约 `-0.654304`；Fps=30 第三帧约 `-0.212037`，显示固定子步/插值路径与 delta 路径不同。 |

`reference.json` 的首帧先调用 Pose 初始化，随后按 Motion→Pose→Core 顺序推进；`motion_loop.json` 的循环由探针显式开启，因为此 Framework 的 `CubismMotion::Create` 不直接按 motion3 的 `Meta.Loop` 开启循环。以上数值来自指定 fixture、初态和 dt 序列，不推断其他模型的默认行为。

`motion_loop.json` 的 `time` 是 MotionManager 的累计时间，不是 clip 局部时间。首次 `UpdateMotion(0.25)` 才建立 motion 起点，因此表中的 manager 时间 `1.5` 对应本次播放从起点经过约 `1.25` 秒；比较循环边界时必须保留这个起点与相同的 dt 序列。

GPU opacity 报告位于 `target/animation-opacity-gpu-probe/report.json`，记录八张 128×128 官方 Framework OpenGL 图、输入哈希、中心/左侧像素和差异范围。本机 Core 6.0.1、OpenGL `2.1 Metal - 91.7` 下，基础图与仅改变 Model opacity 的图逐像素相同，中心 RGBA 都为 `[165,172,181,223]`；显式 renderer alpha=0.5 时为 `[82,86,90,112]`。仅把 Part0 opacity 设为 0.5 时，差异只在左侧 Part0 的像素范围，另一个 Part 的中心像素不变。

静态 Offscreen fixture 从同一自建模型导入后加到 Part0，由本仓库 MOC3 编码器生成，官方 GPU probe 确认 `offscreen_count=1`。Part0 未降透明度时，Offscreen opacity=0.5 的整张图与无 Offscreen、Part0 opacity=0.5 的图逐像素相同；左侧样本均为 `[31,36,47,112]`。再把 Part0 opacity 降为 0.5，左侧样本为 `[16,18,24,56]`，右侧 Part 不变，表明此场景两处 alpha 各应用一次。仅设置 Model opacity=0.5 仍不改变 Offscreen 图像。这些结论限于这一个无 mask、无嵌套的静态 Offscreen。

## JSON 数值 parser 限制

`inline_numbers.motion3.json` 保留第一次失败的**标准合法 JSON**：`Segments` 行内数组最后一个数字紧邻 `]`。标准 Python JSON parser 接受它；本机 Framework 报 `Json parse error : @line 17`、`Invalid Json document`，退出码非零。正向 `minimal.motion3.json` 将数字数组逐项换行后可加载。源码 `Utils/CubismJson.cpp::ParseNumeric` 的数字终止符分支只接受逗号或换行；本机 `--json-check` 还实际观察到紧邻 `}`、数字后空格、指数记法 `1e-7/1e7` 均被拒绝，而零、负数、小数、逗号或换行结束的数字被接受。

无指数十进制极值也需数值检查：接近 `1e-38` 的字面量解析为 `1.00000106e-38`，相对标准 float32 约偏 `1.12e-6`；接近 float32 上限的字面量相对偏约 `1.20e-7`。`numeric.json` 留存实际读数、float32 基准和误差。验收器给这两个已观察极值 `2e-6` 相对漂移界限，只用于发现参考行为变化，**不表示这一区间可安全导出**。后续 exporter 应输出无指数的有限十进制，保证数字后立即是逗号或换行，并用实际 Framework parser 回读验证每个数值；超出约定精度的值需诊断，不能只因 JSON 语法有效便发布。具体可接受值域留给 codec 阶段依据更多样本确定。

## 验收边界

运行器对来源 ID、Core 数量、每帧字段/类型/有限数、帧数、关键数值使用严格断言；负控制包括错误 ID、缺帧/缺字段、布尔值冒充数字、字符串、NaN/Infinity、截断 JSON、零值误读与旧 `passed` 报告。场景数值容差为绝对 `2e-5`，用于本轮短序列 CPU float 对照；极值 parser 误差单独报告。若验收条件漂移，`report.json` 状态为 `failed`，不保留上次成功状态。

尚未验证宿主使用 Model opacity 的最终合成策略、mask/嵌套 Offscreen、Bezier restricted/unrestricted 的动态行为、复杂 physics rig 或长序列累积误差。后续阶段不得把这些 `not_run` 项目视作参考通过，也不得用当前短序列容差代替长序列误差预算。
