# SDK 渐进使用实验

本工具测量陌生 agent 使用已安装 SDK 完成任务的能力。它与
[SDK 正确性验证](VALIDATION.md)分开：正确性测试通过是实验前提，受试实验观察
可发现性、误用、恢复与交付。首版不绑定模型供应商，不调用付费模型，不把主持人
控制脚本的运行算作 agent 成绩。

## 已实现的任务

| ID | 目标 | 验收 |
| --- | --- | --- |
| `create` | PNG 新建单 mesh 工程 | 画布、纹理内容、几何、UV、三角形、绘制属性、保存重开 |
| `parameter` | 新建并添加 Open 参数和两端关键形态 | 上述检查，加参数定义、绑定、端点和中点求值 |
| `edit` | 修改已有双 mesh 工程的一个终点关键形态 | 目标变化，所有其他公开对象状态保持，三档求值 |
| `delivery-transfer` | 导入、局部修改、保存并导出模型包 | 搬迁后重开、语义等价、空目录重放和同根重复交付 |
| `resource-recovery` | 恢复失联纹理并导出模型包 | 纹理内容匹配、诊断、搬迁、重放和重复交付 |
| `visual-locate` | 从参考图定位偏移部件 | 目标几何、观察帧与参考逐像素一致、搬迁重开 |
| `visual-parent` | 修正旋转父级下的局部关键形态 | 父级坐标、几何、观察帧与参考逐像素一致 |
| `compose-expression` | 一个参数驱动嘴部和两侧标记 | 三个绑定、五档采样、端点图像与可编辑交付 |
| `handoff-revision` | 接手前关作品并缩小标记位移 | 保留嘴部和绑定身份、五档采样、端点图像与搬迁 |
| `shirousagi-repair` | 用真实 PSD 和组合姿态参考诊断已交付模型 | 局部修复、未公开姿态、非目标保持、工程与模型包搬迁 |
| `shirousagi-blink` | 从真实 24 层 PSD 创作双眼独立眨眼 | 双参数运行时 ID、公开与未公开组合姿态、其余图层保持、工程与模型包搬迁 |

任务、规则在 `tools/sdk_experiment/tasks.json` 和 `worker.py` 中版本化。
新建任务允许自行选择 UUID、资源名称、顶点编号、对角线和工程输出文件名；
局部编辑任务要求保留原对象身份。浮点绝对容差为 `1e-6`，拒绝 NaN/Infinity。
资源保存后的路径迁移不算内容改变。准备每个任务时运行正控制，以及错误名称、
错误几何的负控制；参数/编辑任务还有错误关键形态控制，编辑任务另有未修改控制。
任何控制失效都不会生成可发放的 `trial.json`。

视觉任务需要支持 Observer 的 GPU wheel 和可用的图像观察环境；`handoff-revision`
需要初始化时冻结一个已通过 `compose-expression` 的工程。任务控制脚本不能算作
受试 agent 成功率，十一项任务也不代表 SDK 的全部使用情境。

## 1. 构建并冻结实验

从仓库根目录运行，先完成现有 SDK 回归验证，再构建候选 wheel：

```sh
uv build --wheel --python 3.14 --out-dir target/python-wheels modules/kasane-python
uv run --locked python tools/sdk_experiments.py init \
  --experiment /tmp/kasane-study-v1 \
  --wheel /absolute/path/to/kasane.whl
```

Shirousagi 和其他视觉任务需要 Observer wheel；构建时使用
`uv run --locked maturin build --manifest-path modules/kasane-python/Cargo.toml --release --features observe --out target/python-wheels`。

接手任务在 `init` 时另加 `--handoff-project /absolute/path/to/passed/project`，
该目录须含 `project.kasane.json`；底座将其冻结，并在准备任务包时复制到 `input/project`。
真实 Shirousagi 任务另加 `--shirousagi-root /absolute/path/to/Shirousagi`，冻结
model3、MOC3、纹理和 24 层 PSD；`shirousagi-repair` 的 `prepare` 用
`--variant a|b|c` 选择故障变体。`shirousagi-blink` 的三个任务包来自同一 PSD，
各自重新导入并冻结对象身份。

先在仓库根目录执行 `uv sync --locked`。实验目录必须在源码仓库外，且不能已存在。
`init` 用 `uv venv --python 3.14` 创建本轮实验共用的环境，通过 `uv pip install`
离线安装给定 wheel，从仓库外检查真实导入，并记录 uv 版本。
冻结 wheel、两份公开文档、任务定义、worker、CLI 和素材；记录各文件 SHA-256、
Git revision、工作区 dirty 状态、Python/平台/SDK 版本及能力、安装后包内容哈希。
Git revision 本身不能代表未提交的代码；具体实验身份以冻结材料与 wheel 哈希为准。

后续命令会核验冻结材料、当前 CLI 与安装的 SDK。修改任一实验条件后建立新实验，
不混写旧成绩。初始化失败的目录保留日志，请换新目录重试。
本工具的进程组超时清理面向 POSIX（macOS/Linux），尚未支持 Windows。

目录布局：

```text
study/
  .venv/                       主持人与本轮所有受试者共用的 uv 环境
  host/
    lock.json                  实验条件
    frozen/                    冻结 wheel、文档、判分器等
    trials/<trial>/
      trial.json               受试条件、输入哈希
      oracle.json              主持人预期状态和控制结果
      agent/                   外部 runner 的命令、输出、耗时、退出状态
      assessments/<id>/        每次验收、交付快照、新目录重放及日志
      reviews/<id>/            主持人轨迹审阅，不覆盖历史记录
  packets/<trial>/            只向受试者开放这个目录和 SDK 环境
    TASK.md
    docs/
    input/
```

## 2. 准备受试包

```sh
uv run --locked python tools/sdk_experiments.py prepare \
  --experiment /tmp/kasane-study-v1 \
  --trial create-01 --task create \
  --model exact-model-id \
  --model-config '{"reasoning":"medium","runner":"your-runner-version"}' \
  --cohort fresh --budget 900
```

把命令返回的 `TASK.md` 发给全新上下文的受试 agent。模型标识与配置由主持人声明，
底座不会替你验证实际供应商配置。固定模型、推理设置、工具集和预算后再开展一批运行。
`fresh` 表示无先前任务经验；`learning` 表示保留经验，必须分别分析。

任务要求交付 `solution.py --output <绝对目录>`，可带辅助 Python 文件，
并先在任务包 `output/` 实际运行。`result.json` 只声明真实工程 manifest 的
绝对路径；可放在 output 根或交付子目录。判分器重新打开工程，不信任受试者自报的检查结果。工程输出布局可自选，
但必须在声明的 output 根目录内。脚本从自身目录寻找 input，不依赖旧输出。

受试者可以安装第三方依赖，所有受试者共用本轮 `.venv`：

```sh
uv pip install --python /tmp/kasane-study-v1/.venv/bin/python pillow
```

不要求每位受试者创建 uv 项目或提交独立锁文件。安装耗时计入预算，在 notes.md
注明依赖用途；被测 kasane 的版本及内容保持冻结。底座在准备、运行前后、验收与
审阅时记录 `uv pip freeze` 清单。工作目录重放仍使用这个共享环境，不重建依赖。
因此这些结果衡量的是共享环境中的可用性，不代表从干净依赖环境重新安装的能力。
依赖变化会影响后续受试者；要比较同一依赖条件的结果，可按记录的清单分组。

## 3. 记录一次 agent 会话

已有自己的模型 runner 时，用 `run` 包住它，命令参数放在 `--` 之后：

```sh
uv run --locked python tools/sdk_experiments.py run \
  --experiment /tmp/kasane-study-v1 --trial create-01 -- \
  /absolute/path/to/your-agent-runner
```

每个 trial 只能启动一次测量会话；agent 自己的查错与重试发生在此会话内。
runner 的工作目录是任务包，接收以下环境变量：

| 变量 | 含义 |
| --- | --- |
| `KASANE_TASK` | 任务 Markdown 的绝对路径 |
| `KASANE_PACKET` | 任务目录 |
| `KASANE_PYTHON` | 本轮共享环境的 Python |
| `KASANE_UV` | 创建实验时使用的 uv 可执行文件 |
| `KASANE_TRACE` | 推荐输出完整工具轨迹的 JSONL 路径 |

runner 应读取任务、配置指定模型、执行工具循环，并把可观察的调用、返回、异常、
时间和人工干预写入轨迹。不要把 API key 放进命令行参数，因为 argv 会被记录。
工具直接记录外部进程 stdout/stderr、真实墙钟耗时、退出码和超时；超时终止进程组，
保留部分日志。日志文件持续写入，可直接查看，不需等待会话结束。

**stdout/stderr 不是完整工具轨迹。** SDK 调用次数、token、费用、首次有效产出时间等
需要 runner 提供结构化数据，首版不猜测这些指标，也不要求隐藏推理内容。
建议轨迹事件含 `timestamp`、`event`、`tool`、`arguments`、`result/error`；
用 `human_hint` 事件记录提示，结束时可附供应商报告的 token/费用。

也可以在桌面 agent 会话手动使用任务包，随后执行 `assess` 和 `review`。
这能得到作品及重放结论，但没有 `run` 计时证据时，不会自动声明预算内无提示成功。

**隔离边界：** 仓库外目录和复制重放提供工作区分离，不是 OS 安全沙箱。
同一系统用户仍可能读写 host/、原任务及源码。正式受试应由 runner 的文件权限、
容器或账户权限隐藏主持人和其他任务。共享环境允许安装依赖；SDK/docs/input 仍应保持不变。
只靠任务提示无法证明没有泄漏；需要结合完整工具轨迹审阅。
重放也不是恶意脚本沙箱，仅执行经过审阅、属于本次受试的脚本。

## 4. 验收、重放和审阅

```sh
uv run --locked python tools/sdk_experiments.py assess \
  --experiment /tmp/kasane-study-v1 --trial create-01
```

保存一次不可覆盖的验收记录及提交快照，核对输入和文档，独立验收原作品。
然后复制交付内容到新目录（排除旧 output、缓存和虚拟环境），用共享 Python
与空输出重跑同一脚本并再次验收。报告保留重放前后的依赖清单。
重放超时默认为 180 秒，可用 `--timeout` 指定；它是验证预算，不是受试预算。
输入变化、缺失 notes.md、原作品失败、重放失败均不能通过。判分基础设施异常标记 `harness_error`。
返回码：0 通过；1 运行或验收未通过；2 配置/启动前置条件错误。

主持人审查实际调用轨迹、脚本及 notes.md 后，记录提示数量和公开 API 使用情况：

```sh
uv run --locked python tools/sdk_experiments.py review \
  --experiment /tmp/kasane-study-v1 --trial create-01 \
  --reviewer reviewer-name --human-prompts 0 --public-api yes \
  --trace /absolute/path/to/full-agent-trace.jsonl \
  --notes '已审阅全部工具调用；记录实际观察到的阻碍及恢复方法。'
uv run --locked python tools/sdk_experiments.py summarize \
  --experiment /tmp/kasane-study-v1 \
  --output /tmp/kasane-study-v1-summary.json
```

审阅是人工声明，程序不自动判断任意 Python 是否只使用公开 API。不要仅凭空日志
或 agent 自报填写“无提示”。每次审阅保存轨迹副本和提交哈希。

`artifact_status` 只表示工程和重放结论。`independent_success` 只有在测量会话、
验收和审阅对应相同提交，且当前提交未变化时才有布尔值；运行与验收之间的依赖
清单不一致也会标为证据不足（`null`）。后续受试者安装依赖不会改写已完成的历史结论。
进程启动失败和判分基础设施错误也不转换为受试失败。超时属于预算内任务失败。
不跨模型、任务、预算、cohort 自动汇总成功率。重判保留全部历史，汇总指向最新报告。
哈希用于检测变化，不是针对有权修改主持人文件的对抗性签名机制。

## 5. 每轮迭代

本仓库已有 S0–S3 和窄范围 S4 的历史受试记录。当前续轮使用三位 Luna subagent：
S3-A 各做一次 fresh，后续依次进入 S3-B、S4-A、S4-B、S5-A、S5-B，均记为
learning；同关作品和重放稳定后才推进。设计见
[续轮计划](experiments/NEXT-ROUND.md)，实际证据、修正与限制见
[2026-09-24 续轮记录](experiments/ROUND-2026-09-24.md)。后续真实素材任务设计见
[Shirousagi 实验计划](experiments/SHIROUSAGI-PLAN.md)及
[S6 结果](experiments/SHIROUSAGI-ROUND.md)。

小批量运行用于发现摩擦，不能估算一般用户成功率。按 SDK 实现、接口设计、文档、agent 环境、任务/判分器
分别归因；从成功轨迹中也寻找反复误用和绕路。

每个问题记录：

```text
问题 ID / 关联 trial 和具体轨迹位置：
可观察障碍及影响：
归因与改进假设：
拟修改内容：
预期变化和验收方法：
原任务复测结果：
新任务迁移结果：
正确性回归结果：
结论：保留 / 调整 / 撤回
```

一批运行结束后再改版。对重要改动用旧/新版本、相同任务和配置、全新会话对照；
分开报告，不改写旧成绩。把已修复问题加入回归任务，同时保留未用于日常调优的任务。

## 底座自身验证

无 SDK 时验证日志、超时、证据不可覆盖和符号链接边界：

```sh
uv run --locked python -m unittest discover -s tests -p test_sdk_experiments.py -v
```

完整验证包括独立安装 wheel、基础与续轮任务的正负控制与重放、输入篡改、
错误几何、缺失/越界 manifest、旧审阅不能给新提交背书，以及受试者安装依赖后
其他受试者和重放脚本能在共享环境使用它：

```sh
SDK_EXPERIMENT_WHEEL=/absolute/path/to/kasane.whl \
  uv run --locked python -m unittest discover -s tests -p test_sdk_experiments.py -v
```

CI 在 macOS/Linux wheel 构建后运行完整底座验证。以上测试使用确定性的控制脚本，
测试结果不能作为受试 agent 成功率。
