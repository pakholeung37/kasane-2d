# motion3 wire 基线

`kasane-live2d::motion3` 只处理 JSON 与 typed 曲线，不持有工程 UUID 或播放状态。`Curves[].Segments` 解码为首点和 Linear、Bezier、Stepped、InverseStepped 四种片段；编码重算 Meta 的曲线、片段、点、事件及 UTF-8 字节数。导入时保留原计数，并用 `counts_match()` 指出不一致。

数值、时间顺序、duration/fps、restricted Bezier 控制点与事件范围经过检查。未知字段保留；包含未知数值的字段暂阻止导出，因为本地 Framework 数字解析约定尚未对这些扩展字段验证。编码器给数组末尾数字留换行，避免 Framework 对部分合法紧凑 JSON 的解析缺陷。

运行 `uv run --locked python tools/run_framework_motion_wire_probe.py` 可重编码仓库最小与 loop fixture，再用本地官方 Framework 的 MotionBehavior V1/V2 CPU trace 比对原文件与编码文件。另一个 fixture 覆盖四类 segment 与中文事件，官方 Framework 验证其计数和 UTF-8 字节长度。该检查证明 wire 编码与这些 fixture 的行为一致；时间轴编辑、事件播放和整包导出仍属 P3 后续工作。

`kasane-animation::sample_motion_curve` 是无状态曲线采样入口。仓库分别保存 restricted 与 unrestricted Bezier fixture；官方 Framework 的九个时间点与 Rust 采样的绝对误差小于 `2e-5`。Linear、Stepped、InverseStepped 的边界另有 Rust 测试。队列淡化、循环 V2、事件发送与 seek 不属于这个函数的保证范围。
