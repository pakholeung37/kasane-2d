# SDK 可用性任务 01：局部修改已有模型

你拿到一个已保存的 Kasane 工程。请只使用已安装的 `kasane` Python 包及本目录的 `SDK-API.md`，编写脚本修改工程。

输入工程：`{baseline_manifest}`

目标：找到**唯一名称为 `right` 的 mesh**，找到它的参数绑定。该工程只有一个参数 `expression`，取值范围为 0–1。把 `expression=1` 的 keyform 中每个顶点的**父级局部坐标 X 增加 0.1**，Y 不变。`expression=0` 的 keyform 不变。保留所有对象 ID、其他 mesh、资源、层级、绘制属性和绑定字段。

把结果保存为新工程，放在 `{output_directory}` 下；不要修改输入工程。交付：

1. 可重复执行的 `solution.py`。
2. 保存后工程的 manifest 绝对路径。
3. 一小段说明：你如何找到目标、验证结果，以及遇到的 SDK 阻碍。

可以使用公开的 Python API、`help(kasane)` 和运行时签名检查。不要读取仓库源码、现有 recipe、测试或判分器；不要直接改工程 JSON、调用私有 `_native` 或用导入导出重建整份工程。遇到错误可自行重试并保留错误记录。本任务从第一次打开文档开始计时，限时 25 分钟。
