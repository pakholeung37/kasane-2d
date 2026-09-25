# 动画编辑器实施与验收记录

日期：2026-09-25。实施对象是 Rust/Python SDK；GUI 时间轴仍按规划后置。

## 已接入的资产与编辑路径

- `kasane-live2d` 提供 cdi3、exp3、motion3、physics3、pose3 的 typed codec。导出重建计数并使用已通过官方 Framework JSON parser 验证的数字编码。
- `kasane-core` 的 v6 工程数据（兼容读取并迁移 v1–v5）持久化 DisplayInfo、Expression、Motion/注册组、Pose、Physics、model3 Groups/Layout/HitAreas，以及 Sound/UserData 的受管字节。相关对象进入 checkpoint、撤销、引用检查和大小估算。
- `kasane-project` 在候选工程中导入附件，发布时生成 model3 及资源；缺失附件在工程中持久记录并阻止整包发布。编辑器可在修复后或明确决定舍弃时逐项清除记录。未知字段与无法安全重写的 runtime ID 变更执行严格导出保护。
- Rust SDK 和 Python wheel 暴露相应创建、替换、导入、导出与预览入口。Motion 预览按 Motion → Expression → Physics → Pose 执行，播放状态不修改工程；`seek` 从初态或标准时间格点检查点按 60 Hz 重放输入与激活时间表，进度回调可取消且不改变取消前状态。Physics 另有独立预览入口，支持 reset 与 stabilization。
- `kasane-sdk-observe` 可直接捕获已求值动画帧，并检查预览所属 session、generation 与 revision。宿主可显式选择把 Motion `Model/Opacity` 作为 Framework renderer color alpha 应用于 Drawable；Offscreen 自身的 opacity 独立计算。Framework 默认不自动应用 Model opacity。

## 参考验证

从仓库根目录运行：

```sh
cargo test --workspace
cargo clippy -p kasane-animation -p kasane-core -p kasane-live2d -p kasane-project -p kasane-sdk -p kasane-python -p kasane-sdk-observe --all-targets -- -D warnings
python3 tools/run_framework_animation_cpu_probe.py
python3 tools/run_framework_cdi_probe.py
python3 tools/run_framework_expression_probe.py
python3 tools/run_framework_motion_wire_probe.py
python3 tools/run_framework_physics_sequence_probe.py
python3 tools/run_animation_package_probe.py
uv run --locked python tools/run_animation_gpu_probe.py
```

Physics 差分覆盖缺省、零、30 FPS 和串联双 rig，每种 120 帧与 stabilization 后 20 帧。报告位于 `target/physics-sequence-probe/report.json`。联合包 probe 先导出模型，复制到独立目录并移除原包，再核对所有 model3 相对引用及官方 Framework 的 model3、CDI、Expression、Motion、Physics、Pose 加载；报告位于 `target/animation-package-probe/report.json`。

GPU probe 取 Motion/Pose 的 0.25、0.50、0.75 秒，分别在普通模型、单层 Offscreen、嵌套 Offscreen 上对照官方 OpenGL 与 Kasane WGPU；每帧再比较宿主显式把 Model opacity 设为 renderer color alpha 的变体，共 18 张 128×128 图。全部通过，每通道最大像素误差为 1/255。报告及图片位于 `target/animation-frame-gpu-probe/`。此探针读取 Rust 预览采样的参数与 Part opacity，官方 GPU 探针以相同数值绘制；Motion/Pose 的时间求值由独立官方 CPU 差分验证。它覆盖选定场景，不能替代所有宿主 GPU 与资源包组合。

Python wheel 验证：

```sh
uv build --wheel --python 3.14 --out-dir target/python-wheels modules/kasane-python
cd /tmp
uv run --no-project --no-cache --python 3.14 --with /absolute/path/to/kasane-0.1.0-cp314-cp314-macosx_11_0_arm64.whl python /absolute/path/to/kesane-2d/modules/kasane-python/tests/test_cpu.py
```

上面的绝对路径应替换为当前仓库 `target/python-wheels` 中实际 wheel 路径。
本机 CPython 3.14 安装 wheel 后的 CPU 测试为 47 项通过；`cargo test --workspace` 与上面的 Clippy 检查通过。

## 边界

- 整包导出发生在无宿主 GPU 的 project 层，因此包内 `export-report.json` 的 `framework_render_validation` 仍为 `not_performed`；独立 GPU 报告位于上述探针目录。两个报告分别描述发布时检查与后续像素验收。
- Motion 的自动 EyeBlink/LipSync Model 曲线动态映射按规划后置。预览 `coverage` 会报告未覆盖项；Parameter 曲线照常执行。

## 2026-09-25 工作区 Review 修正

- 修复非循环 Motion 缺少自然结束时间，导致 clip/track 淡出失效的问题；PartOpacity 写入真实参数时遵循 Framework 的范围限制。
- 修复大步长跨多个循环时事件漏发；曲线时间改用余数计算，避免逐周期减法。预估事件批次超过一百万时返回 `EVENT_LIMIT`，且不修改预览状态。
- 将轨道到运行时曲线的转换移到预览创建阶段，移除逐帧复制全部 segment 的分配。
- 发布前统一检查生成文件与附件的路径空间，拒绝大小写冲突、文件与目录冲突及非规范路径。允许 Sound 与生成的 Motion 文件共用目录，并同步附件所有父目录，保证发布前的持久化检查完整。
- 保留 model3 未知 FileReferences 字段，并阻止严格导出静默丢弃这些依赖。
- 新增 Rust 边界回归测试，扩展独立附件打包测试至 `motions/audio/`；Python 同步覆盖循环事件、错误码及失败原子性，并校正自然淡出的预期值。

本次修正后验证：`cargo test --workspace`、上述七个 crate 的 `clippy --all-targets -- -D warnings`、独立包 probe 全部通过；重新构建 CPython 3.14 wheel 后在源码目录外运行 CPU 测试，48 项通过。Framework CPU probe 的 CPU 检查通过，其总报告仍为 `partial`，因为该 probe 的像素与 Offscreen GPU 检查为 `not_run`；本次 Review 未重跑 GPU 差分。


## 架构重构与破坏性 API 变更

- `MotionPreview` 统一持有时间与输出快照；`MotionRuntime`、`ExpressionRuntime` 只处理对应阶段的队列与求值。独立 Expression 预览使用同一个阶段实现。两种 seek 复用绝对 60 Hz 时间格点，组合 seek 共享文档与编译曲线，仅重建可变状态。
- `CompiledCurve` 使用领域轨道数据，采样算法不再接收 motion3 wire 对象。`sample_motion_curve` 的 Rust 参数改为 `&CompiledCurve`。Physics 定义和不变量迁入 Core，JSON 编解码留在 live2d；Core 与 animation 都不再依赖 live2d。
- Motion 文档存储使用 `Arc<MotionClip>`；track segments 使用 `Arc<Vec<MotionSegment>>`。checkpoint 共享资产，修改一条轨道时采用 copy-on-write。历史预算仍保守计入每份快照可达的共享内存，不低估预算。
- `build_export_plan` 捕获所有生成文件与受管资源字节、SHA-256、文件类型和引用边。`publish_export_plan` 在包级验证后发布相同字节，保留原有 stage/backup/rollback 契约。Rust `ArtifactValidator/ArtifactValidation` 替换为 `PackageValidator/PackageValidation`；回调接收完整 `ExportPlan`，报告分别记录 Core、model3 loader、animation、render 的验证状态。
- model3 Groups、Layout、HitAreas 改为 typed records；已解析的 Parameter/Mesh 引用使用 UUID，未解析引用显式保留 runtime ID。结构校验、删除保护、undo/redo、大小估算与持久化均接入。未知扩展及 UserData 的保守 namespace 保护继续保留。
- 工程写 v6、读 v1–v6。v5 原始 model3 JSON 在 codec 边界按已保存的 namespace 迁移，随后使用 typed 结构；旧 reader 必须拒绝 v6。Python Groups/HitAreas 的字典形状改为领域字段，示例见 `modules/kasane-python/API.md`。

存储基准可运行 `cargo run --release -p kasane-animation --example storage_benchmark`。本机一次样本：64 条轨道、每条 2048 segments、100 次操作，checkpoint 约 0.084 ms、单轨道 copy-on-write 约 0.491 ms、模拟整 clip segment 深复制约 19.320 ms。该合成基准只衡量存储复制路径，不代表包含校验、渲染和 GUI 的整体交互耗时。


本次架构重构验证：`cargo test --workspace`、`cargo test -p kasane-project --features binary-prototype`、七个相关 crate 的严格 Clippy 全部通过；重新构建并安装 CPython 3.14 wheel 后，49 项 CPU 测试通过。Expression 官方差分、Physics 长序列、独立资源包 probe 均通过。重新执行 18 帧 GPU 差分通过，每通道最大误差仍为 1/255；其覆盖范围及独立报告位置与上文相同。

## 提交后边界复查

修复 Expression 越界目标值在淡入期间的限幅顺序：与 Framework `CubismModel::SetParameterValue` 一致，先对目标值限幅，再与基准值按权重混合。新增回归覆盖 Add、Multiply、Overwrite 以及独立/组合预览；官方 expression probe 增加三种模式的越界差分。全工作区 Rust 测试、animation 严格 Clippy 和扩展后的官方 probe 均通过。

注册条目级预览和 seek 检查点缓存已补齐，Rust SDK 与 Python 同步开放：

- `schedule_motion_entry(group, index, time)` 按零起始索引选择 model3 注册条目。每次激活保留独立 fade override，优先级为轨道 > 注册条目 > clip > 默认 1 秒；同一 clip 的不同注册不会互相污染。Sound 仍是资源元数据，CPU 预览不播放音频。
- 组合 `MotionPreview` 每隔 60 个标准回放步（1 秒）保存完整检查点，含各阶段状态、物理粒子/输入缓存/余时、队列游标和事件快照。部分尾步、`seek(0)`、任意 `advance` 和 stabilization 不生成检查点。独立 ExpressionPreview 保持从头回放。
- 默认缓存预算 16 MiB，`set_seek_cache_budget(bytes)` 可调整或以 0 禁用，`clear_seek_cache()` 可清空；`seek_cache_stats()` 报告保守估算占用、检查点数、恢复时间与本次回放步数。超预算按插入顺序淘汰，单个过大的检查点直接跳过。预算覆盖保留缓存，排除共享文档和原子 seek 的临时工作状态。
- 成功修改激活/输入时间表、基准值或 reset 会使缓存失效。取消或回调异常保留原来的播放状态和缓存；精确命中时仍调用 `(0, 0)`，允许取消。进度总数是命中后实际剩余回放步数。
- 完整状态回归比较缓存与禁用缓存的前后跳转、分数时间、零时刻和事件结果；60 秒后 seek 到 61 秒仅回放 60 步，禁用缓存为 3660 步。

本轮验证：`cargo test --workspace`、animation/sdk/python 的严格 Clippy、重建 CPython 3.14 wheel 后的 50 项 Python CPU 测试通过。官方 CPU probe 的已执行检查全部通过；报告仍为 partial，因为该 CPU probe 不运行像素与 offscreen GPU 检查。本轮未修改 GPU 渲染路径。
