# SDK 最小素材

`asymmetric-2x2.png` 是可分发的 2×2 RGBA PNG，四角依次为红、绿、蓝、半透明黄。S1 纵切测试用它检查 PNG 尺寸和 SHA-256 描述；后续 S4 观察测试会用非对称颜色确认 UV 方向与透明度。

`authoring-contract.json` 是 Rust SDK 与 Python wheel 共用的创作契约输入及预期结果，覆盖失败回滚、建模求值、重命名和 undo/redo。

SDK 首轮创作可用性实验的任务、判分与记录口径见 [实验方案](../../docs/SDK-USABILITY-EXPERIMENT.md)。
五次独立试验的结果与使用障碍见 [实验结果](../../docs/SDK-USABILITY-RESULTS.md)。
第二轮 Luna 类型检查对照见 [第二轮结果](../../docs/SDK-USABILITY-ROUND2-RESULTS.md)。
使用 Python wheel 写脚本时，可先看 [类型速查](../../docs/SDK-PYTHON-TYPING.md) 并运行 Pyright。
