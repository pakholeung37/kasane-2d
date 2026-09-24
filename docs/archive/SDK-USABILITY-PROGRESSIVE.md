# SDK 渐进可用性实验

此实验检验陌生使用者能否仅凭已安装 wheel 和公开文档完成常见工作流。历史局部编辑实验见 [首轮结果](SDK-USABILITY-RESULTS.md) 和 [类型检查对照](SDK-USABILITY-ROUND2-RESULTS.md)；历史成绩不与新梯度混算。

## 梯度

| 关卡 | 常见需求 | 独立验收重点 |
| --- | --- | --- |
| S0 | 从 PNG 新建单 mesh 工程 | 环境、素材导入、画布坐标、保存、重开 |
| S1 | 添加参数及两个关键形态 | 绑定、插值、端点与中点求值 |
| S2 | 局部修改已有工程 | 对象发现、父级局部坐标、非目标保持、保存冲突 |
| S3 | 导入/导出交付包 | 资源完整性、导出结构、可重新打开 |
| S4 | 观察渲染并修正 | GPU 能力探测、图片证据、视觉诊断与可复现报告 |

仅在当前关稳定后实现并开放下一关任务。S0、S1、S3 和 SDK 改版复测的准备与判分入口是 `tools/sdk_usability_harness.py`；S2 复用已有的 `prepare_sdk_usability.py` 与 `evaluate_sdk_usability.py`。后续关卡共用隔离目录、环境锁定、过程记录和评分口径，但各自有独立判分器。每关结束先将反复出现的障碍归因到 SDK API、SDK 错误、环境或实验工具；能由 SDK 消除的障碍优先修改 SDK 并构建新 wheel，再由新的受试者复测。

## 运行协议

1. 固定 Git revision、CPython 3.14、`uv` 版本和 wheel SHA-256。用 `RUSTFLAGS='-C strip=none' uv build --wheel ...` 构建；`uv venv` 和 `uv pip install` 创建与安装隔离环境。先从仓库外运行 `import kasane` smoke test。原生模块加载失败算 harness 环境故障，不算受试者失败。
2. 对同一关运行主持人正、负控制。每位 Luna subagent 使用独立新任务目录，只得到任务包、公开文档和已安装的 Python 路径；不提供源码、示例、判分器或别人的轨迹。主持人固定任务文本与 wheel 后再发任务。
3. 受试者交付脚本、结果 manifest 和错误笔记。主持人用独立判分器验证作品，用 Pyright 检查静态接口，用全新输出目录重跑脚本；核对素材和文档哈希，并审查是否使用了禁用入口。记录实际命令、异常、耗时和人工提示。自报时间单独标记，不用来代替命令轨迹。
4. 每位受试者按 100 分记：作品状态 60（每项判分检查平均分配）、从空目录重跑 20、Pyright 0 错误 10、过程与阻碍记录 10。违反任务约束、修改输入或使用私有实现时，作品状态分记 0，并单独保留原始状态判分以定位问题。
5. 当前关的门槛：pilot 正控制通过、负控制失败；两位独立 Luna 都无人工提示，作品状态全通过、从新目录重跑通过、没有输入改动，且总分至少 90。此门槛只用于工程迭代，不推断人群成功率。若修复改变 wheel、任务或语义判分，先复验控制，再给两位受试者新的任务包重跑同一关，不把修复前后的分数合并。若仅删除任务未写出的隐藏路径限制，可在保留原始报告的同时，对原始作品重新判分并复验正负控制。稳定后才前进。

## S0 本机命令

```bash
RUSTFLAGS='-C strip=none' uv build --wheel --python python3.14 modules/kasane-python --out-dir target/sdk-usability/progressive/wheels
uv venv --python python3.14 target/sdk-usability/progressive/.venv
uv pip install --python target/sdk-usability/progressive/.venv/bin/python target/sdk-usability/progressive/wheels/kasane-*.whl
target/sdk-usability/progressive/.venv/bin/python -c 'import kasane; print(kasane.capabilities())'
target/sdk-usability/progressive/.venv/bin/python tools/sdk_usability_harness.py prepare-s0 /absolute/packet /absolute/host/setup.json --python /absolute/.venv/bin/python
target/sdk-usability/progressive/.venv/bin/python tools/sdk_usability_harness.py grade-s0 /absolute/host/setup.json /absolute/packet/output/result/project.kasane.json --report /absolute/host/grade.json
```

`target/sdk-usability/progressive/` 保存原始包、控制实验、两位受试者的脚本和判分报告，不纳入 Git。任务中的 manifest 文件名以实际保存结果为准。
