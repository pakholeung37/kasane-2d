# S6 Offscreen 与扩展混合验收（2026-09-21）

**S6 已通过：统一入口的 9 项必需门禁全部 passed，源码和原生库哈希一致。** 最终报告为 `target/kasane/m3c/s6_report.json`；专项通过不能替代完整门禁。此记录覆盖图像正确性、正式 Editor 工作流、混合模式及资源预算，独立应用打包仍属于 S7。

## 根因与修复

1. **目标颜色快照过期。** Godot 的 `hint_screen_texture` 自动快照不会在每个扩展混合命令前重新生成。后续全表面 `blend_disabled` 合成读取旧目标，连透明区域也写回旧内容，覆盖之前已合成的普通子表面。每次扩展 Mesh / Offscreen 绘制前插入复用的 `BackBufferCopy`，并按 Core render plan 排序。禁用第二次复制的负向对照会重现兄弟图层丢失。
2. **实际消费视口依赖缺失。** 场景 sibling 顺序不等于 RenderingServer 的依赖顺序。用 `viewport_set_parent_viewport` 登记 Offscreen、Mesh mask、Offscreen mask 的消费视口，使子表面在同帧先于父表面完成。ready 依赖真实 pre/post draw 提交，而非简单等待帧数。
3. **坐标和 shader uniform 类型。** 重父级必须使用 `keep_global_transform=false`。离屏裁剪边界对齐屏幕整数像素，root 使用完整仿射变换、composite 使用逆变换；shader `mat3` 通过 `Basis` 传递，不能传 `Transform2D`。后者在单位变换构造测试中未暴露，却导致缩放后的瞳孔遮罩消失。Offscreen 遮罩保留至少模型像素密度（最长边上限 4096），避免小预览的低分辨率遮罩造成眼缘误差。旧 Mesh 遮罩保持已验收的采样策略。[Godot uniform 类型对照](https://docs.godotengine.org/en/stable/tutorials/shaders/shader_reference/shading_language.html)
4. **直 Alpha / 兼容模式语义。** Mesh 纹理的 opacity 和 mask 只缩放 source alpha；Offscreen 是预乘结果，缩放 RGBA。AddCompatible / MultiplyCompatible 按官方兼容规则保留目标 alpha、忽略 Alpha selector，不能套用扩展 Alpha 公式。
5. **v6 再导出排序。** v6 的排序组后代总数必须包含 Offscreen；仅计算 Mesh 会使官方 Core 再导出文件的 render order 冲突。v6 encoder 已修复，旧格式路径保持原有计数规则，Ren 导出回归包含运行时排序对照。

## 资源契约

- 每层按“变换后的模型画布与目标视口的交集”分配，并对齐物理像素；不再逐层分配 5200×7000 纹理。
- Viewport、Mesh、ShaderMaterial、目标复制节点按对象 ID 复用。禁用或无有效绘制的子树保留节点，纹理缩至 Godot 的最小 2×2 并停止更新；重新启用恢复同一组节点。
- 删除 Offscreen 前迁移其存活子节点，避免延迟释放连带删除 Mesh。已有遮罩网格删成零三角形时，同时清除 MeshView 和两种 mask 路径中的 GPU 几何；删空再恢复有独立像素回归。相机平移、旋转、缩放、目标尺寸变化和跨视口重父级均刷新依赖与坐标。
- 分配前检查合计 **512 MiB** 附件预算：每个活动表面按两份 RGBA8 预留，另加 Mesh / Offscreen mask。单纹理最长边不超过 4096；超限返回 `OFFSCREEN_BUDGET_EXCEEDED`，不修改既有分配。
- `get_render_stats()` 的 `offscreen_color_bytes` / `offscreen_reserved_bytes` 是尺寸估算；后者不含 mask，mask 单列。`gpu_texture_bytes` / `gpu_video_bytes` 是 RenderingServer 实际计数，不能冒充驱动总显存。
- Ren 2048×2048 回归逐帧采集 RenderingServer 内存，整个测试进程的渲染资源峰值门限为 **768 MiB**（包括 atlas、主视口等）；另用 `ps` 记录该进程 RSS。二者均不代表其他进程或驱动全部分配。稳定帧要求纹理内存、创建和 resize 计数不增长。

## 本次资源专项实测

Godot 4.7.2 / Compatibility / Apple M4，2048×2048 Ren 预览，24 个持久 Offscreen 节点、默认姿态 17 个活动表面、198 个 Mesh、246 条命令：

| 项目 | 实测或估算 |
|---|---:|
| Offscreen 颜色附件（尺寸估算） | 163,862,296 bytes，156.3 MiB |
| 两份颜色/目标附件预留（尺寸估算） | 327,724,592 bytes，312.5 MiB |
| Mask 颜色附件（尺寸估算） | 346,912 bytes |
| RenderingServer 逐帧渲染资源峰值 | 578,684,137 bytes，551.9 MiB |
| 测试进程 RSS 采样峰值 | 416,792,576 bytes，397.5 MiB |

稳定刷新 30 帧的纹理内存、创建及 resize 计数不增长；缩放 A→B→A 图像一致，参数 ready 图像与继续等待后的结果一致。4096×4096 超预算请求在分配前被拒绝，随后恢复提交成功。具体样本、adapter 和门限见 [资源报告](../target/kasane/m3c/s6/ren-resources/godot-report.json)。

## 固定 SDK 的模式矩阵

参考为仓库固定的 `CubismSdkForNative-5-r.5`。原始编码为 `color | alpha << 8`，高 16 位必须为零。

| Color 值 | 名称 | Color 值 | 名称 |
|---|---|---|---|
| 0 | Normal | 9 | Lighten |
| 1 | AddCompatible | 10 | Screen |
| 2 | MultiplyCompatible | 11 | ColorDodge |
| 3 | Add | 12 | Overlay |
| 4 | AddGlow | 13 | SoftLight |
| 5 | Darken | 14 | HardLight |
| 6 | Multiply | 15 | LinearLight |
| 7 | ColorBurn | 16 | Hue |
| 8 | LinearBurn | 17 | Color |

| Alpha 值 | 名称 |
|---|---|
| 0 | Over |
| 1 | Atop |
| 2 | Out |
| 3 | ConjointOver |
| 4 | DisjointOver |

共 18×5＝90 个合法编码组合。Color 1/2 的 Alpha selector 是兼容别名，因此实际有 82 种不同语义。矩阵逐组合运行 24 个样本，共 **2,160** 项：Mesh 直 Alpha / Offscreen 预乘 Alpha、透明/半透明/不透明源与目标、颜色端点、opacity、普通及反向 mask。每一项独立判定，不以矩阵整体平均掩盖某模式失败。

## 独立图像 oracle 与正式应用流程

`tools/probes/framework_gpu_probe.cpp` 直接链接官方 Core 和未修改的 Framework OpenGL renderer，使用真实 mask / Offscreen manager 渲染原始和新导出 MOC。混合矩阵直接编译 SDK 原始 Color/Alpha GLSL 函数；两个兼容模式使用官方固定混合等式。SDK 源文件不被修改或复制成生产实现。

验收保持 RGBA8、相同 atlas mipmap / 线性采样、相同参数值和 Editor 实际相机。全图、前景、上/中/下区域，以及 **每个活动 Offscreen 的几何 bounds ROI** 均采用同一阈值：RGBA MAE ≤ 0.005，任一通道误差 > 0.05 的像素占比 ≤ 1%。ROI 由当前几何决定，不能按差分图挑选；瞳孔组独立判定，防止小面积缺失被全身平均值掩盖。

正式应用测试 staging 当前 `apps/editor` 与本次编译的 debug GDExtension，运行真实 Workspace、Hierarchy、Inspector、UndoRedo、ProjectIO 和 Canvas。测试窗口关闭人工输入干扰，脚本仍通过公开编辑接口执行：

- Ren：导入 → 参数 +15 → Offscreen opacity / multiply 编辑 → Undo/Redo → Inspector 定位 → 保存 → 删除源文件副本 → 重开 → 新 MOC 导出 → 参数 -15。原始和导出文件分别通过官方 renderer 比较。
- Mao：原文件对照 → 四类 BlendShape、约束、关键值表、Glue、拓扑和 Undo/Redo 的既有 M3B 编辑脚本 → 保存 → 删除源文件副本 → 重开并验证结构保留 → 导出 → 官方图像对照及新增 Glue 参数 0.65 对照。
- macOS 遮挡窗口可能停止自动绘制但继续 `process_frame`。`observe` 在等待时可请求 `RenderingServer.force_draw(false)`，仍由实际绘制信号认证，不伪造 ready。

## 重跑与报告

macOS / Compatibility GPU；需要 Rust、CMake、固定 SDK、Godot 及安装 numpy/Pillow 的 Python。仓库不附带 SDK 的再分发资产。

```sh
python3 tools/validate_m3c.py --stage S6
python3 -m unittest discover -s tools -p test_validate_s6.py
```

本机使用的 Python：`/Users/pakholeung/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/bin/python3`。

统一入口真实执行构建、混合矩阵、目标复制负向对照、生命周期、Ren 资源、M4 数据/GPU、M3B 双 Core 数值和正式 Editor 官方图像门禁。任一缺失、失败或未运行均返回非零；运行前先清除旧成功状态，报告保存源码/SDK/库 SHA-256，运行中源码或库发生变化也不能通过。`--matrix-only` 只输出 `S6-matrix` 专项，不能宣称 S6 完成。

主报告：[s6_report.json](../target/kasane/m3c/s6_report.json)。各子报告、PNG、差分、Editor 请求结果及逐帧内存位于 `target/kasane/m3c/s6/`。M3B 子项仅引用 `numerical_status`；该子报告的里程碑总状态仍为 `incomplete`，不会将数值通过冒充旧打包应用验收。当前源码应用流程由本阶段的 `editor_official` 单独验证。

## 最终验收结果

| 门禁 | 结果 |
|---|---|
| 当前 GDExtension / 官方 Framework oracle 构建 | passed，源码、SDK 与库 SHA-256 已记录 |
| 混合矩阵 | 2,160 / 2,160 passed |
| 目标复制和嵌套合成 | passed，含负向对照 |
| 生命周期、遮罩和动态几何 | 106 / 106 passed |
| Ren 资源与稳定帧 | passed，含超预算拒绝及恢复 |
| M4 Godot 数据集成 | 151 / 151 passed |
| M4 旧混合 GPU 回归 | 84 / 84 passed |
| M3B 双 Core 数值 | 147 批、3,505 组参数样本，两个 Core 均 passed |
| 正式源码 Editor / 官方 Framework | 7 组图像、103 项全图及局部检查 passed |

本次还通过 111 项 Rust 测试、workspace check 和 5 项验收脚本单元测试。当前 `apps/editor/native/libkasane_godot.dylib` 已同步到报告中的 debug 库，源码应用启动检查无错误；此前已打开的 Godot 进程需重启加载。S7 独立应用打包和全版本发布闭环未在本阶段宣称完成。
