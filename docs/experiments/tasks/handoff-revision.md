# S5-B：接手并修改表情作品

`input/project/` 是另一位作者交付的可编辑 Kasane 工程；此任务包不提供作者脚本或
完整会话记录。请按 `input/request.json` 的追加需求接手：保留 `mouth` 的
`Expression` 行为，把左右标记在参数终点相对起点的上移幅度改为每个顶点 3 个
源画布像素。保留标记的起点、工程中的其他对象、资源、参数、绑定身份与父级关系。
`input/reference-0.png`、`input/reference-1.png` 是追加需求的两端目标图。

交付 `solution.py --output <绝对目录>`、可重新打开的工程及 `notes.md`。
脚本从自身目录寻找 input，不依赖作者会话或旧输出；在输出根内写 `result.json`，
记录实际绝对路径 `project_manifest`、`observation_report`、`contact_sheet` 与按
`input/request.json` 中五档采样顺序排列的 `frames`。`result.json` 可以放在输出根
或单次交付子目录，多份时主持人读取最新写入的一份。

请在任务包 `output/` 实际运行，并确认新空输出目录重放成功。主持人将独立检查
0、0.25、0.5、0.75、1 的几何与固定 Observer 像素，以及工程保存、重开与搬迁。
数值绝对容差 `1e-6`。可按任务卡环境说明用 uv 安装 Pillow 等依赖。
只通过公开 kasane API 修改工程；不得编辑工程 JSON、MOC3、纹理、参考图或 input，
不得读取主持人预期结果、作者脚本、其他任务包或源码仓库。
