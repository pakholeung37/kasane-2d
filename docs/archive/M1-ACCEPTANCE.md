# M1 验收与复现

2026-09-20：在提交 `871c5b158a98f8f87c899187935279ec0831656a` 上重新执行独立验收，通过。Core 全新构建的 8 项 CTest（含 Purism unit、验证器负例与 C99 bundle）、59 项 Godot 集成检查、84 项真实 GPU 检查全部通过；gd-cubism 与 kasane-gd 的 SCons 构建成功。双 Core 最大数值误差分别为 0 与 `7.15256e-7`。环境为 macOS arm64 / Apple M4 / Godot 4.7.2 / OpenGL Compatibility。报告位于 `target/kasane/core-regression/report.json`、`target/kasane/godot-boundary/report.json`、`target/kasane/gpu-regression/report.json`。本次沿用下述范围与阈值；首次动态导入退出问题仍不计为通过。

2026-09-19：本机 M1 统一验收通过。代码创建、编辑、内存求值、MOC3 导出及运行包路径已贯通。原始证据保存在 `target/kasane/runs/<run-id>/report.json`；这是当时的验收快照，后续回归以各项独立检查的结果为准。

本次结果：7 项 CTest、59 项 Godot 集成检查、84 项 GPU 检查与 C99 bundle 均通过。两个 Core 各比较 7712 项数值；Purism 最大误差 0，官方 Core 最大误差 `7.15256e-7` 运行单位，最大位置误差 `0.000071526` 原画像素。三个状态的图像及局部裁剪逐像素一致；独立解析像素预期最大通道误差 `0.002048`，低于 `2/255`。

## 现在如何复现

本机验收配置：macOS arm64，Godot 4.7.2 mono，OpenGL Compatibility，640×480，Apple M4 GPU。Core、Godot 和 GPU 回归分别运行；GPU 检查需要真实图形环境。

```sh
python3 -m venv target/kasane/buildenv
target/kasane/buildenv/bin/python -m pip install -r tools/requirements-validation.txt
# 系统需要 CMake、C/C++ 编译器和 libpng 开发库；SDK 路径按本机安装位置调整。
target/kasane/buildenv/bin/python tools/validate_core.py
target/kasane/buildenv/bin/python -m SCons -C modules/gd-cubism platform=macos arch=arm64 target=template_release CUBISM_SDK_ROOT="$PWD/third_party/CubismSdkForNative-5-r.5" -j8
target/kasane/buildenv/bin/python -m SCons -C modules/kasane-gd platform=macos arch=arm64 target=template_debug -j8
target/kasane/buildenv/bin/python tools/validate_godot.py
target/kasane/buildenv/bin/python tools/validate_gpu.py
```

`validate_core.py` 可用 `--sdk /absolute/path` 指定本地 SDK；Godot 和 GPU 脚本可用 `--godot /absolute/path` 指定 Godot。默认 SDK 为 `third_party/CubismSdkForNative-5-r.5`；实际 Core 版本从报告读取，不能由目录名推断。SDK 不提交到仓库。各检查失败时返回非零。

检查内容：

1. 构建 kasane-core、编码器和通用发布器；运行 CTest（包含两个 Core、Purism unit、验证器负例与 C99 bundle smoke）。Purism 外部模型 conformance 需另外提供模型与参考数据。
2. 重建官方 Core 的 gd-cubism 参考播放器及 kasane-gd。
3. Godot headless 源数据、预览生命周期、工程快照和失效原子性检查。
4. 启动真实 GPU 窗口，比较现有 gd-cubism 官方 Core 播放与直接消费 Document 的 KasaneDocumentPreview。

## 验收对应关系

| M1 必需项 | 证据 |
|---|---|
| 两纹理静态网格 | 双 Core 检查画布、UV、非连续顶点 ID 编译、拓扑、纹理槽与资源描述 |
| 普通参数与组合 | 单参数三形态及中点、非对称 3×3、2×2×2、轴顺序独立、钳制、单关键值、边界外禁用 |
| 嵌套变形 | Rotation→Warp、Warp→Rotation，三角/quad、非零原点、缩放/双轴反射、网格外推；关键值和中点 |
| Part / 绘制结构 | 父子 Part、动态 Part 顺序、Mesh 顺序、透明度、multiply/screen、三种混合、普通/反向遮罩 |
| 编辑后再导出 | 形态修改、重绑定、删除、完整拓扑替换和顶点映射、画布更新；不相关 Mesh 不变 |
| 无效数据 | 缺/重复组合、重复 ID、未知引用、关系环、非有限值、非法索引/拓扑、不可表示 ID、无效版本/截断/offset/count |
| 文件失败保留 | 缺 PNG、PNG 损坏/尺寸错误、验证器拒绝、不可写目标，旧包字节保持一致 |
| 源数据持久化回归 | 参数、Mesh/Part/变形器属性与 Keyform 重开一致；预览值不保存，失败不替换源或句柄 |
| 实现复用 | Core ABI 不变，关键值搜索、组合、向量混合、Rotation/Warp、父级方向算法共享，原有 Purism 回归与 bundle 通过 |
| GPU | 3 次连续姿态/相机状态、9 个绘制区域，整图、每个对象局部裁剪及独立解析像素预期；84 项检查 |

GPU 使用统一的线性/mipmap 过滤、透明背景、画布和相机。生成实际图、参考图、原始差异图及局部裁剪，1350 个像素位置、两条渲染路径的解析比较记录到 `pixel-samples.json`。阈值沿用 [统一规则](VALIDATION.md)，未放宽：整图/裁剪 MAE≤0.005、大误差像素≤1%，确定区域每通道≤2/255。解析预期独立计算纹理方向、颜色、透明度和混合方程，避免两个 renderer 同时出现相同错误。

## 产物位置

当前独立检查分别写入 `target/kasane/core-regression/`、`target/kasane/godot-boundary/` 和 `target/kasane/gpu-regression/`。以下路径属于 2026-09-19 的历史整体验收记录；删除总入口不会删除已有记录。

- `target/kasane/runs/<run-id>/report.json`：整体状态、所有门禁、源码指纹、环境与 SHA-256。
- `target/kasane/runs/<run-id>/core/report.json`：数值/编码/编辑/发布验证。Core 子报告不单独代表 GPU 验收。
- `target/kasane/runs/<run-id>/core/package*/`：静态、1D/2D/3D 参数和四种嵌套构造模型包。
- `target/kasane/runs/<run-id>/core/{official,purism}/publication/gpu-package/`：由正式发布器生成的多层/遮罩运行包。
- `target/kasane/runs/<run-id>/godot/report.json`：源/预览/持久化集成检查。
- `target/kasane/runs/<run-id>/gpu/`：参考、实际、差异及裁剪 PNG，参数/相机采样与像素明细。

构造模型是由源码 API 新建的合成测试资产，纹理由程序生成，不冒称来自 Cubism Editor 的外部模型。官方 Core 加载、驱动和 Godot 播放证据不等于测试了 Cubism 官方 Viewer 应用。

## 已知限制与后续里程碑

Godot 4.7.2 mono 在首次扫描并动态载入扩展后退出可能崩溃，重构前 HEAD 同样复现。验收项目在启动前登记扩展，稳定加载；这没有修复引擎首次扫描问题。详见 [重构说明](M1-CORE-REFACTOR.md)。

预览适配器暂以普通 ShaderMaterial 和独立遮罩 viewport 接收 DrawableFrame。现有 renderer 的几何/参数纹理、mask atlas 和 owner 都绑定 CubismModel，尚不能直接接入 Document；本轮提供最小适配并用 GPU 严格对照。M4 负责将它们统一，当前不宣称公共 renderer 已完成。此次还修正了参考播放器无遮罩 multiply shader 的 blend mode。

M2 的工程搬移/素材打包、M3 外部 MOC3 导入、M4 renderer 提取、M5 编辑器 UI/完整脚本体验均未被 M1 验收替代。容量、坐标域和拒绝规则见 [字段映射](formats/MOC3-WRITER.md)。
