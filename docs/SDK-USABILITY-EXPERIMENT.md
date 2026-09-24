# SDK 创作可用性实验：第一轮

第一轮独立试验已完成，结果见 [SDK-USABILITY-RESULTS.md](SDK-USABILITY-RESULTS.md)。第二轮以三个 Luna subagent 对照类型检查步骤，结果见 [SDK-USABILITY-ROUND2-RESULTS.md](SDK-USABILITY-ROUND2-RESULTS.md)。

## 要回答的问题

现有 [SDK 阶段验收](SDK-ACCEPTANCE.md) 已证明本机 wheel 的功能、数值和图像路径可运行。第一轮可用性实验问另一个问题：第一次使用 SDK 的 agent，拿到已保存的工程和公开文档后，能否**找到正确入口、局部修改、验证并交付可重复的脚本**？

先测外部工程局部编辑，因为它会同时暴露对象发现、绑定快照、坐标语义、原子编辑、保存重开和错误诊断中的障碍。下一轮再测从素材创建与依据图像证据修正；不要用仓库中已写好的 S5 recipe 代替受试者完成任务。

## 第一轮任务与成功条件

准备器生成 `TASK.md`、`SDK-API.md`、已保存的 `baseline/` 工程、空 `output/` 与记录输入 hash 的 `setup.json`。主持人先把 `setup.json` 复制到受试者无法修改的位置，再把任务目录交给受试者。任务正文见 [task-01.md](../examples/sdk/usability/task-01.md)。不提供源码、现有 recipe、测试和判分器。使用相同 CPython 3.14 wheel，CPU 即可完成。下一轮任务只新增 Pyright 检查命令；[Python 类型速查](SDK-PYTHON-TYPING.md)可供日常开发使用，但不能在同任务对照试验中泄露操作答案。

2026-09-24 的第一轮**未**提供 Pyright 步骤；原始任务包已保存在本地证据目录。现在的任务模板加入类型检查，供下一轮使用，成绩不能直接与第一轮合并。

目标是只调整名称为 `right` 的 mesh 在唯一参数 1.0 处的 keyform：父级局部 X 全部增加 0.1。输入工程必须原样保留；输出工程需保存并可重开。受试者交付 `solution.py`、manifest 路径和简短过程说明。允许公开 Python API、包帮助和运行时签名检查；禁止直接改 JSON 或调用私有原生对象。

独立判分器 `tools/evaluate_sdk_usability.py` 用已安装 wheel 分别打开输入和输出，检查结构与资源、对象 ID、非目标内容、目标 keyform、参数端点以及另一 mesh 的采样结果。它忽略保存到新目录时正常变化的资源路径与快照版本。**自动通过只证明作品状态**；主持人还要检查脚本能重跑、没有使用禁用入口，以及输入工程未被修改。

## 实验过程

1. 主持人固定一个 wheel hash、Python 版本和环境；在仓库外新建 venv，安装 wheel。记录 SDK 文档版本与 Git revision。若不测 GPU，就不提供 GPU feature，避免设备状态干扰。
2. 每位受试者使用新的独立目录运行准备器：`python tools/prepare_sdk_usability.py /absolute/unique/run`。只把该目录交给受试者；主持人保存准备前后 baseline 文件 hash。
3. 从受试者第一次打开 `TASK.md` 开始计时 25 分钟。不得主动提供方法名或实现提示。受试者可自行查询公开文档、运行脚本、读取 SDK 错误和重试。主持人逐次记下卡点、异常代码、查阅位置和任何干预。
4. 对交付的 manifest 运行：`python tools/evaluate_sdk_usability.py /absolute/run/baseline/<manifest> /absolute/run/output/<manifest> --setup /host-only/setup.json --report /host-only/grade.json`。判分器退出码 0 为状态通过；无交付、超时、判分器错误或未满足约束均单独记录。
5. 在一个新的输出目录再次运行 `solution.py`，复核它没有依赖第一次试验的临时状态。检查 baseline hash 不变。最后访谈：“最难找到的入口是什么？”“哪条错误信息没有告诉你下一步？”“你对结果正确性还有什么不确定？”

先用一个非盲 pilot 检查准备器和判分器的正、负控制；pilot 不计入可用性结果。之后至少进行 5 次独立、无提示的 agent 试验。模型与推理设置、可用文档和限时保持一致；如修改 SDK 或文档，另起一轮，不合并前后成绩。这是发现障碍的小样本实验，不作为总体成功率的统计估计。

## 记录表与判断口径

每次保存以下字段，不能只记录最终“通过/失败”：

| 字段 | 记录方式 |
| --- | --- |
| 环境 | Git revision、wheel SHA-256、Python/OS、受试 agent 型号与设置、任务目录 |
| 时间 | 首次查看任务、首次成功打开工程、首次有效 edit、第一次判分通过、结束；用单调时钟或命令日志核对 |
| 过程 | 执行命令数、失败尝试数、SDK 错误 code、文档/帮助查询位置、人工提示次数与内容 |
| 产物 | 脚本、保存后的工程、grader JSON、输入工程前后 hash、重跑结果 |
| 障碍 | 按 API 发现、数据形状、坐标/语义、错误信息、持久化、环境、任务表述分类；附具体证据 |

预先采用的首轮目标：5 次中至少 4 次在 25 分钟内**无提示**交付可重跑且通过判分的脚本；成功运行的中位完成时间不超过 15 分钟；没有输入工程修改或非目标对象改动。未达目标时，先读失败轨迹和错误分类再决定改 API、文档或任务说明。达到目标仍需进入“从素材创建”和“图像诊断修正”两轮，不能据此宣称 SDK 整体可用。

## 可复现命令

在仓库根目录、使用已安装 CPU wheel 的 Python：

```bash
python tools/prepare_sdk_usability.py /absolute/new-trial-directory
# 主持人在开放任务目录前保存一份独立元数据
cp /absolute/new-trial-directory/setup.json /absolute/host-only/setup.json
python tools/evaluate_sdk_usability.py \
  /absolute/new-trial-directory/baseline/project.kasane.json \
  /absolute/new-trial-directory/output/project/project.kasane.json \
  --setup /absolute/host-only/setup.json \
  --report /absolute/host-only/grade.json
```

实际 manifest 文件名以 `TASK.md` 和受试者交付为准，不要假设上面的示例名称。准备器从现有两素材 recipe 生成 fixture，但不会把 recipe 或其报告放进任务目录。判分器只使用公开 `kasane` API；程序性通过后仍需人工审阅过程约束。

## 工具 pilot（2026-09-24）

使用从 `01ec274` 的 SDK 代码构建的 CPython 3.14/macOS arm64 CPU wheel，SHA-256 为 `4d229792857f3461986ba8cbcd2269c736712f2ee47ff69c9efa9762dbb5677a`。准备器产出的输入工程在独立目录重开后没有结构或资源诊断，任务目录不含 recipe 或其报告。第一次运行发现 macOS 的 `/tmp` 与 `/private/tmp` 路径别名，已通过规范化源路径修复。

非盲正控制：使用公开 API 只移动目标的 1.0 keyform，判分器 `passed`。负控制一：复制原工程而不修改，`one_keyform` 和 `evaluated_one` 失败。负控制二：完成目标修改但额外重命名另一 mesh，`meshes_content` 失败。输入工程 hash 由 `setup.json` 保存并参与判分。以上只检验实验工具；**尚无独立受试 agent 的完成时间、错误轨迹或成功率**。
