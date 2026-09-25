# Mao 动画集成验收

日期：2026-09-25。输入为本地授权样例
`models/local/mao/runtime/mao_pro.model3.json`；样例文件在 `models/local/`
下，不随 Git 提交。产物位于 `target/mao-animation-integration/`。

## 复现

先构建 Python wheel 和已有的官方 Framework CPU probe，然后从源码目录外运行：

```sh
uv build --wheel --python 3.14 --out-dir target/python-wheels modules/kasane-python
python3 tools/run_framework_animation_cpu_probe.py
python3 tools/run_framework_cdi_probe.py
cd /tmp
uv run --no-project --no-cache --python 3.14 \
  --with /absolute/path/to/target/python-wheels/kasane-0.1.0-cp314-cp314-macosx_11_0_arm64.whl \
  python /absolute/path/to/tools/run_mao_animation_integration.py
```

报告为 `target/mao-animation-integration/report.json`；官方 Viewer 打开
`target/mao-animation-integration/live2d-model/model.model3.json`。同目录另有
`mao-live2d-model.zip`，解压后从模型根目录打开 `model.model3.json`。

## 本次结果

- 导入 7 个 Motion、8 个 Expression、128 个参数和 260 个 Drawable；保存工程重开后数量与 model3 注册组不变。
- 按 model3 注册条目播放全部 7 个 Motion，并单独播放 8 个 Expression；组合 Motion、Expression、Physics、Pose 后重复定位同一帧，状态一致。所有帧的参数有限且能生成 260 个 Drawable。
- 与官方 Framework MotionBehavior V2 对照 7 条 Motion、每条 7 帧的第一个参数，最大绝对误差约 `2.15e-6`。官方 Framework 接受全部 7 个 motion3、8 个 exp3、model3、CDI、Pose；Physics 执行 120 帧。
- 导出包中的 JSON 动画、Physics、Pose、CDI 与源文件语义一致（浮点编码允许 `1e-5` 绝对误差），贴图字节一致。模型包复制到独立目录后，所有相对引用存在，并可重新导入。
- Expression 和 Motion 保留可读文件名：`exp_01`–`exp_08`、`mtn_01`–`mtn_04`、`special_01`–`special_03`；对应 model3 引用与原始样例一致。

真实样例暴露并修复三类兼容边界：末帧时间比四舍五入后的 Motion Duration 多 `0.003` 秒；model3 使用空 Motion 分组名和空 HitArea 显示名；CDI 与 Motion 包含不在 MOC 中的虚拟通道。导入仍报告 16 条诊断（14 条参数、2 条 Part），预览 `coverage` 标出未绑定通道，导出保留其原始 runtime ID。它们在当前 MOC 中无 Drawable 目标。用户于 2026-09-25 确认官方 Viewer 视觉验收通过；自动化报告仍仅记录脚本运行时的检查。
