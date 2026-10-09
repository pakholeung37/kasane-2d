# Shirousagi：整模 Y 轴变形与语义化 Parts

日期：2026-09-24。延续[头部 X 轴工程](SHIROUSAGI-HEAD-X.md)，本轮脚本是 [`shirousagi_head_xy.py`](shirousagi_head_xy.py)，输出在 `output/head-xy-experiment/result/`。输入是上轮可编辑工程；24 个 PSD 图层及其 X 轴绑定、素材、绘制状态均保留。

## 视觉参考边界

本轮只把交付模型加载进 `Observer`，渲染 `ParamAngleY=-30/0/+30` 及中间、X×Y 组合姿态，和本工程 RGBA 图片比较。**不读取交付模型的参数定义、关键形态、mesh 顶点或变形器**。上轮 X 轴工程是既有输入，属于上轮几何迁移实验；本轮没有继续从交付 rig 求解 Y 的内部数据。

整模 Y 轴采用一个 10 行、1 列的共享 warp。各控制行的位移在 768×768 参考图上通过逐行图像对齐起稿，再以本工程渲染图与参考图的像素差调整。控制点按 `ObservedFrame.view_scale/view_offset` 换回本工程画布坐标。所有 24 个 mesh 及既有 X keyform 均换成 warp 的 parent-local 坐标；Y=0 时保持原画面。嘴部的三层共用一个子 warp，按参考图修正相对脸轮廓的上下位置。双耳共用另一个子 warp，使耳尖和耳根相对头部有独立的 Y 位移。Y 轴装配共用三个变形器及三个 scene binding，无需新增 24 个逐层 Y binding。

第二次检查发现：X 轴的耳朵和脸各有独立 mesh binding；Y 轴之前只有整模 warp，`Part` 仅整理层级，**不会自动赋予差分运动**。现将两耳网格和 X keyform 转入“双耳 Y 轴相对位移”子 warp。子 warp 在中立姿态与父 warp 的控制行逐点对齐，不改变原画面；抬头时耳尖额外下移约 5 个预览像素，耳根约 2 个像素，保持连接并产生可编辑的组级差分。[耳朵修正前后图](../../output/head-xy-experiment/result/ear-y-before-after.png)展示了同一姿态。

用户在大尺寸预览中指出 `Y=+30` 时头脸被压扁。旧版虽然更接近交付模型的像素，但共享 warp 把画面第 310–390 行压到原高度的 **43.3%**，随后又拉伸脸部。新版重新分配抬头端点的位移，限制各段最低纵向比例为 **81.25%**；脸更圆，眼睛与脸颊的间距更自然。[同一视角的修改前后图](../../output/head-xy-experiment/result/head-up-before-after.png)可直接对照。此修正以用户指出的形态问题为准，允许与交付参考图的像素差上升。

## 语义标签与组合

保留 `ArtMesh` runtime ID 供导出和外部驱动识别；24 个显示名称全部改为 PSD 画面对应的中文名称。16 个 Part 构成以下组织树，具体逐层映射见脚本的 `LAYERS` 和输出 `report.json`：

```text
白兔｜整体
├─ 身体（躯干）
└─ 头部
   ├─ 双耳（画面左耳、画面右耳）
   ├─ 脸部轮廓
   │  ├─ 腮红
   │  ├─ 嘴部
   │  ├─ 双眼（左右眼及隐藏替换层）
   │  ├─ 鼻尖
   │  ├─ 眉毛替换层
   │  └─ 两侧表情替换层
   └─ 额头装饰替换层
```

Part 的 `draw_order` 显式沿用 PSD 原有层序。初次分组时，脸轮廓曾盖住眼睛和嘴；结构诊断对此并不报错。调整 Part 顺序后，Y=0 的 X 单轴画面与上轮工程相同：`X=-30/-15/0/15` 逐像素相同，`X=30` 仅有极少量数值舍入差。

## 图像结果

在 Apple M4 上以 768×768 渲染，指标是全幅 RGBA 四通道平均绝对差，范围 0–255。表中“原 X 工程”是未加入 Y 变形的上轮工程；参考是交付模型**渲染图**。

| X | Y | 原 X 工程 → 参考 | 新工程 → 参考 |
| ---: | ---: | ---: | ---: |
| 0 | -30 | 9.7756 | 2.1125 |
| 0 | -15 | 5.6321 | 1.1277 |
| 0 | 0 | 0.2454 | 0.2454 |
| 0 | 15 | 4.2410 | 1.9639 |
| 0 | 30 | 7.0816 | 3.4942 |
| -30 | -30 | 9.2113 | 2.3037 |
| 30 | 30 | 7.6323 | 4.0914 |

参考图与结果图：[`Y=-30` 结果](../../output/head-xy-experiment/result/candidate-xp0-ym30.png)、[`Y=-30` 参考](../../output/head-xy-experiment/result/reference-xp0-ym30.png)、[`Y=+30` 结果](../../output/head-xy-experiment/result/candidate-xp0-yp30.png)、[`Y=+30` 参考](../../output/head-xy-experiment/result/reference-xp0-yp30.png)。其余姿态与像素计数见 [`report.json`](../../output/head-xy-experiment/result/report.json)。

画面接近，但不是交付 rig 的逐像素复刻。尤其 `X=30,Y=30` 的耳、头顶、颈部仍有可见轮廓误差。当前共享 warp 主要表达整模的纵向伸缩和位移；独立遮挡或轮廓形变仍需进一步按美术目标制作。这里的误差是视觉验收证据，不被当成 SDK 结构错误。

## 交付与复现

工程含 24 mesh、23 个既有 X mesh binding、16 Part、3 warp、3 个 Y scene binding 和 `ParamAngleX/ParamAngleY` 两参数。结构、资源、几何诊断均为空。14 个 X/Y 姿态在可编辑工程重开、异目录搬迁、package 重导入和 package 搬迁后与新工程逐像素相同；导出保留两个参数的 runtime ID。

先按 [`modules/kasane-python/README.md`](../../modules/kasane-python/README.md) 安装带 `observe` feature 的 wheel，并完成上轮 X 工程，然后在仓库根目录运行：

```sh
.venv/bin/python docs/experiments/shirousagi_head_xy.py \
  --input-project output/head-x-experiment/result/project \
  --output output/head-xy-experiment/new-run
```

输出目录必须不存在。模型源及生成的工程位于 Git 忽略目录，不提交受许可限制的素材。
