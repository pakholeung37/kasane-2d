# S5-A：多部件表情

`input/project/` 是可编辑工程；`input/request.json` 给出了目标参数、部件、源画布
像素位移、Observer 设置和采样值。请让一个 `Expression` 参数共同驱动嘴部张开、
左右标记上移。两个端点参考图在 `input/reference-0.png` 和
`input/reference-1.png`。只需实现 request 中的定量变化，不以主观审美评分。

保留原有对象的身份、基础几何、纹理和参数定义。参数值 0 时各部件保持原状；
值 1 时嘴部上边顶点按 request 上移、下边顶点下移，左右标记整体上移。
0、0.25、0.5、0.75、1 应连续插值，不能出现部件消失或非目标漂移。
请通过公开 SDK API 建立可继续编辑的完整 mesh 绑定，不用直接修改工程 JSON。

交付 `solution.py --output <绝对目录>`、工程与 `notes.md`。脚本从自身目录寻找
input，在输出根内保存工程，并写 `result.json`，包含实际绝对路径
`project_manifest`、`observation_report`、`contact_sheet`、按五个参数采样值排序的
`frames`。`result.json` 可位于输出根或单次交付子目录；多份时读取最新的一份。
请在任务包 `output/` 实际运行，并确保新空输出目录重放成功。

主持人会独立重开工程，检查五档几何、全部其他公开对象状态、固定 Observer 像素，
并确认报告和帧对应交付工程。数值绝对容差 `1e-6`。
可按任务卡环境说明用 uv 安装 Pillow 等依赖，不创建独立环境。
不得修改 input、文档、参考图或纹理；不得读取主持人预期结果、其他任务包或源码仓库。
