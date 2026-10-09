# Shirousagi：从 PSD 创建头部 X 轴绑定

后续整模 Y 轴变形、语义化标签与 Part 重组见[头部 XY 实验](SHIROUSAGI-HEAD-XY.md)。

日期：2026-09-24。复现脚本是 [`shirousagi_head_x.py`](shirousagi_head_x.py)，机器报告和可编辑工程保存在 `output/head-x-experiment/result/`。输入是 `models/local/Shirousagi` 中的交付模型及由它转出的 PSD；脚本只通过公开 Python SDK 操作工程。

## 做法与边界

1. `Session.import_psd` 产生 24 个图层、24 个矩形 mesh、24 个 PNG 资产，且无 warning。PSD 自身没有参数、关键形态或变形器。
2. 独立导入交付模型，求值 `ParamAngleX=-30/0/+30` 的 mesh 顶点作为**几何参考**。对原模型三角形内的点使用重心插值，把位移场投影到 PSD 各图层的新网格；网格外使用最近顶点位移。
3. 在 PSD 工程中创建一个 runtime ID 为 `ParamAngleX` 的参数，为除躯干 `ArtMesh30` 之外的 23 个图层建立三态 mesh binding。原模型的 34 参数 rig 和变形器没有复制进新工程。
4. 保存可编辑工程，导出 model3/MOC3/纹理 package；分别重开、重导入、搬到临时目录并检查五个姿态。

这是一种**有已交付模型作参考的几何迁移实验**，不能证明 SDK 能从 PSD 单独推断正确转头形态。它也没有重建原模型的 Y 轴、眨眼、表情等其余参数。

本轮后已将可重复使用的步骤沉淀到 Python SDK：`rectangle_grid_geometry` 生成四角矩形网格；`Session.remesh_rectangle_grid` 对已有普通绑定、mesh BlendShape 和 glue 做原子拓扑迁移；`CanvasSnapshot.runtime_to_source` / `source_to_runtime` 处理画布单位。参考模型位移投影与固定裁剪区域的视觉比对留在实验脚本中，因为目前只验证了这一个角色与一种网格外回退策略。真实 PSD 的五姿态误差与最初手写实现相同。

本例 1,408 个头部网格顶点中有 707 个位于同名参考 mesh 的三角形范围外，采用最近参考顶点位移。许多点处于 PSD 裁剪矩形的透明边缘；这次画面仍通过参考对比，但这个数量说明回退策略必须显式报告，不能把投影成功误解为全部点都有三角形内插值。

## 结果

在 Apple M4 上用当前仓库 `observe` wheel 以 768×768 渲染。头部指标是固定的全宽第 25–509 行，计算四个 RGBA 通道的平均绝对差（0–255）；`变化像素`指该像素任一通道相差超过 2。静态基线是 PSD 导入工程在默认姿态的画面。

| `ParamAngleX` | 静态 PSD → 原模型平均差 | 新绑定 → 原模型平均差 | 静态 PSD → 原模型变化像素 | 新绑定 → 原模型变化像素 |
| ---: | ---: | ---: | ---: | ---: |
| -30 | 8.9302 | 0.2516 | 44,510 | 6,470 |
| -15 | 4.9855 | 0.2619 | 31,843 | 6,602 |
| 0 | 0.2742 | 0.2741 | 6,717 | 6,702 |
| 15 | 6.4929 | 0.2543 | 39,451 | 6,706 |
| 30 | 11.5998 | 0.2709 | 55,918 | 6,939 |

所有 24 层的绘制属性保持不变；默认姿态相对原始 PSD 仅 21 个头部像素超过差值 2。下半身第 540–767 行在五个姿态中与静态 PSD 逐像素相同。结构、资源、几何诊断均为空；可编辑工程重开和异目录搬迁在五个姿态均逐像素相同。导出的 MOC3 保留 `ParamAngleX` runtime ID，资源诊断和 warning 为空。新 MOC3 为 71,488 字节，原 MOC3 为 110,976 字节；新工程 manifest 为 719,433 字节。

导出 package 重导入在 `-30/0/30` 与工程逐像素相同；`-15` 有 14 个像素不同、单通道最大差 1；`15` 有 83 个像素不同、单通道最大差 4。package 搬迁后数值相同。这个差异极小，但“任意中间值严格逐像素一致”的承诺目前不成立。

## 流程经验与系统需求

- **PSD 的信息上限与创作任务**：静态图层没有侧脸、遮挡规则或转头关键形态。此实验使用已交付模型作几何参考；若只有 PSD，受试者需要自行设计形变和必要的侧向美术。网格密度、逐层修形与视觉验收属于创作任务；与原模型不逐像素相同不能直接判为 SDK 缺陷。规则网格生成器解决建网格的机械步骤，不替代密度和形变决策。
- **坐标转换缺口**：`evaluate()` 返回运行时坐标，PSD mesh 使用画布像素坐标；原模型与 PSD 工程的 `pixels_per_unit` 分别为 4000 和 100。初版脚本须显式换算。当前工作树增加的 `CanvasSnapshot.runtime_to_source()` 等辅助方法消除了手写公式，仍需在文档和调用路径中清楚标明坐标空间。
- **可选的自动起稿**：当同时提供参考模型、参数和明确的图层映射时，可以考虑自动生成初始 mesh 与关键形态，供作者编辑。这是特定起稿方式，不是所有图层套用同一网格，也不代替受试者的形变创作与视觉验收。此次位移投影仍由实验脚本实现，网格外采样策略尚未收敛为公共接口。
- **交付往返精度**：中间值在 MOC3 导出/重导入后有少量像素差。可对导出一致性做可重复的数值回归；验收容差应按交付要求定义，不要求艺术姿态与参考模型逐像素相同。

结构、资源、几何诊断负责工程合法性。此次 `Observer` 图像比较是实验自己的参考测量，不是 SDK 对艺术效果的自动判分。这轮没有遇到阻止头部 X 绑定、保存或导出的 SDK 错误。

## 复现

先按 [`modules/kasane-python/README.md`](../../modules/kasane-python/README.md) 构建并安装带 `observe` feature 的 wheel，然后在仓库根目录运行：

```sh
.venv/bin/python docs/experiments/shirousagi_head_x.py --output output/head-x-experiment/new-run
```

输出目录必须不存在。默认只保留可编辑工程、导出 package、报告与 `-30/0/30` 三张预览；加 `--keep-debug` 才保留初次 PSD 导入工程及所有参考、中间姿态图。完整数值见 [`report.json`](../../output/head-x-experiment/result/report.json)，左右端点画面见 [`candidate-m30.png`](../../output/head-x-experiment/result/candidate-m30.png) 和 [`candidate-p30.png`](../../output/head-x-experiment/result/candidate-p30.png)。
