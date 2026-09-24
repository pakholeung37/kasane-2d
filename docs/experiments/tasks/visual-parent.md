# S4-B：旋转父级下的视觉修正

`input/project/` 是可编辑工程。`input/reference-endpoint.png` 是正确工程在
`input/observation.json` 指定的参数值与 Observer 设置下得到的目标图。
当前工程只有**一个 mesh 的该参数终点关键形态位置**与目标不符；该 mesh 位于固定的
非零旋转父级下。其他内容正确。请从图像与公开工程信息定位部件，处理画面、源画布、
父级局部坐标之间的关系，只修改那个终点的几何。不得修改父级变换、参考图、纹理、
其他关键形态、参数、画布或其他场景对象。

交付：

- `solution.py --output <绝对目录>`，输入从脚本所在任务目录寻找。
- 输出根内的可重新打开工程；`result.json` 包含实际绝对路径 `project_manifest`。
- 用同一 Observer 设置渲染参数 0、0.5、1；`result.json` 还需包含实际绝对路径
  `observation_report`、`contact_sheet` 和按参数顺序排列的 3 个 `frames` 路径。
- `notes.md`：说明定位证据、父级与坐标换算、局部关键形态修正量、验证方法、
  遇到的错误和依赖用途。

`result.json` 可在输出根或单次交付子目录；多份时主持人读取最新写入的一份。
先在任务包 `output/` 实际运行。必须能在新的空输出目录重放。主持人会独立重开
工程，用固定 Observer 重渲染，检查几何与像素；自报图像比较结果不作判分依据。
数值容差 `1e-6`。可安装 Pillow 等依赖，使用本轮共享 uv Python，不创建独立环境。
只使用公开 kasane API 修改工程；不得直接编辑工程 JSON、MOC3、纹理或参考图，
不得读取主持人预期结果、其他任务包或源码仓库。
