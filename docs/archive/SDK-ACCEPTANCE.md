# SDK 阶段验收记录

2026-09-24 清理旧宿主后的当前门禁见 [VALIDATION.md](VALIDATION.md)。完整本机复跑使用 release wheel、固定外部 v50 参考图及双 Core 探针，不再启动 Godot；报告位于 `target/sdk-acceptance/1027ef7b7b8d46e7b0b07a1f4928ca58/report.json`，10 项均为 `passed`。本机 macOS 27 的 Rust 1.98 release 链接需要 `RUSTFLAGS='-C strip=none'`，否则 wheel 的 Mach-O `LINKEDIT` string pool 对齐错误会导致 Python 无法导入扩展。以下保留迁移期间的验收过程与当时的运行环境。

本文件记录已经实际运行的 SDK 验收；当前 CPython 3.14/macOS arm64 上的 CPU、真实 GPU、双 Core 数值与图像真值门禁均已通过。其它平台组合尚未验收。

## S2：工程与资源闭环（2026-09-23）

运行环境：macOS，本地 Rust toolchain；所有工程、PNG 和导出产物由集成测试写入独立临时目录，测试结束清理。外部导入测试使用仓库内 `tests/fixtures/external_v50` 的 model3、MOC3 与纹理。

| 命令 | 结果 |
| --- | --- |
| `cargo test -p kasane-sdk -p kasane-project` | 通过：SDK 36 项集成测试、project 38 项集成测试及 1 项序列测试 |
| `cargo clippy -p kasane-sdk -p kasane-project --all-targets -- -D warnings` | 通过，无 warning |
| `git diff --check` | 通过 |

`modules/kasane-sdk/tests/project_io.rs` 的 10 项测试覆盖：保存/重开后的持久内容与 CPU 求值一致；save-as 后同 ID 不同图片的历史资源；无 hash 旧资源的历史提示；已删除资源的历史路径；done/redo 跨保存；保存冲突和失败打开不替换会话；发布后目录同步 warning 的保存基线；model3 与 bare MOC3 导入；导入后编辑、保存和导出；relocate/replace 的内容校验；显式 base 的相对 PNG 路径。

导出沿用 `kasane-project` 的结构验证和 publication 机制。测试确认生成 MOC3 文件，未将 structural pass 解释为官方运行时验收。独立外部模型预检查入口、完整 Python API、GPU 观察和 S5 的图像证据尚未验收。

## S3 首批 Python CPU wheel（2026-09-23）

本地固定 PyO3 0.29.2、maturin 1.15.0，使用 CPython 3.14.3 构建 `kasane._native` 的 macOS arm64 wheel；wheel 安装在仓库外 `/tmp/kasane-sdk-python-venv`，测试从 `/tmp` 执行，`PYTHONPATH` 清空。wheel 内含 `__main__.py`、`_native.pyi` 与 `py.typed`。

| 命令 | 结果 |
| --- | --- |
| `cargo test -p kasane-python -p kasane-sdk -p kasane-project` | 通过：原有 SDK/project 测试和 Python crate 构建测试 |
| `cargo clippy -p kasane-python -p kasane-sdk -p kasane-project --all-targets -- -D warnings` | 通过，无 warning |
| `python3.14 modules/kasane-python/tools/check_coverage.py` | 通过：115 个 Rust SDK 入口均已列出；29 个已绑定，86 个待绑定 |
| 从 `/tmp` 运行已安装 wheel 的 `modules/kasane-python/tests/test_cpu.py -v` | 7 项通过：创建/保存/重开、数组副本、参数插值、导入/导出、rollback、双线程版本冲突、runner 异常行号 |
| `python -m kasane run examples/sdk/python_cpu_recipe.py --report <path>` | 通过：报告 `passed`，创作与保存后 `modified=false` |

这些结果验证了本机 CPython 3.14 的首批 API。尚未验证其它 Python 版本、free-threaded wheel、完整 Rust API 覆盖或 GPU 观察。

同日续测：追加查询、预览和 `new_project` 绑定后重新构建并在同一仓库外 venv 安装 wheel，`test_cpu.py` 9 项通过；`cargo clippy -p kasane-python --all-targets -- -D warnings` 通过。覆盖检查当前为 59/115 已绑定、56 项待绑定。新增测试验证快照数据与版本同时读取、预览修改不影响独立求值、预览失败保留状态、名称查询与缺失错误、对象 ID 列表、历史事件和工程重置。

再续测：加入 canvas/parameter 替换、删除、层级关系编辑与 mesh binding 快照；重新构建安装仓库外 wheel 后，`test_cpu.py` 11 项通过。覆盖清单为 69/115 已绑定、46 项待绑定。层级关系编辑的当前 Python 测试验证失败时同批次回滚，成功路径等待 Part/Transform 创建绑定后补齐。

再续测：句柄有效期、绘制顺序组的提交/拒绝/撤销，以及历史步数上限。仓库外 wheel 的 `test_cpu.py` 13 项通过；`cargo clippy -p kasane-python -p kasane-sdk -p kasane-project --all-targets -- -D warnings` 通过；覆盖清单为 77/115，余 38 项。

Part 续测：同批次创建父子 Part、设置组织父节点与 mesh Part、替换 Part 字段、undo 恢复。仓库外 wheel 的 `test_cpu.py` 14 项通过；覆盖清单为 80/115，余 35 项。

Transform 续测：同批次创建旋转/warp、设置 Transform 父节点和 Part 归属、设置 mesh 变形父节点；更新旋转姿态与 warp 点后 undo。仓库外 wheel 的 `test_cpu.py` 15 项通过，Clippy 无警告；覆盖清单为 84/115，余 31 项。

PNG 资源续测：显式基目录导入，拒绝尺寸不符搬迁，相同内容搬迁，替换新图片后 undo。仓库外 wheel 的 `test_cpu.py` 16 项通过，Clippy 无警告；覆盖清单为 87/115，余 28 项。

几何提示续测：面积及画布范围提示不修改版本，非法阈值返回业务错误。仓库外 wheel 的 `test_cpu.py` 17 项通过；修正后的覆盖检查为 116 项，其中 88 项 Python 绑定、1 项 Rust 专用、27 项待绑定。

SceneBinding 续测：Part、Rotation、Warp 轨道的完整表、外观字段、单 keyform 更新、替换、快照隔离和保存重开。仓库外 wheel 的 `test_cpu.py` 18 项通过，Clippy 无警告；覆盖清单为 93/116 项 Python 绑定、1 项 Rust 专用、22 项待绑定。

MeshBinding 续测：appearance、draw order、单 keyform 更新与完整绑定替换，保存重开后字段及中点采样一致。仓库外 wheel 的 `test_cpu.py` 19 项通过，Clippy 无警告；覆盖清单为 95/116 项 Python 绑定、1 项 Rust 专用、20 项待绑定。

Mesh 绘制属性续测：更新外观、绘制顺序、混合模式与遮罩属性后，几何保持不变；非法混合模式回滚同批次改名；undo 恢复原属性。仓库外 wheel 的 `test_cpu.py` 20 项通过，覆盖清单为 96/116 项 Python 绑定、1 项 Rust 专用、19 项待绑定。

Transform 完整替换续测：rotation 和 warp 的快照字段可替换并保存重开，runtime ID 保留，错误类型组合回滚，undo 恢复旧值。仓库外 wheel 的 `test_cpu.py` 21 项通过；覆盖清单为 97/116 项 Python 绑定、1 项 Rust 专用、18 项待绑定。

Offscreen 续测：创建、快照副本、完整替换、保存重开、非法索引回滚和 undo；单独扩容 Part 表被拒，Part 表与 Offscreen 映射联合扩容成功并可 undo。仓库外 wheel 的 `test_cpu.py` 23 项通过；覆盖清单为 101/116 项 Python 绑定、1 项 Rust 专用、14 项待绑定。

Glue 续测：创建、快照副本、带参数绑定的完整替换、保存重开、无效顶点回滚和 undo。仓库外 wheel 的 `test_cpu.py` 24 项通过；覆盖清单为 104/116 项 Python 绑定、1 项 Rust 专用、11 项待绑定。

BlendShape 元数据续测：blend_shape 参数、key table 与 constraint 创建、替换、快照副本和保存重开；约束长度错误整批回滚，undo 恢复旧表。仓库外 wheel 的 `test_cpu.py` 25 项通过；覆盖清单为 110/116 项 Python 绑定、1 项 Rust 专用、5 项待绑定。

BlendShape binding 续测：六种 target 的 delta keyform 创建、读取；mesh binding 替换、快照副本、保存重开、错误长度回滚及 undo。仓库外 wheel 的 `test_cpu.py` 26 项通过；覆盖清单为 113/116 项 Python 绑定、1 项 Rust 专用、2 项待绑定。

Mesh 全量与拓扑续测：自定义三角 mesh 创建、全字段快照及替换；同时更新顶点 ID、普通 binding、BlendShape binding、Glue，验证不完整映射与陈旧快照回滚、保存重开和 undo。仓库外 wheel 的 `test_cpu.py` 28 项通过；覆盖清单为 115/116 项 Python 绑定、1 项 Rust 专用、0 项待绑定。

S3 CPU 门禁：外部 CPython 3.14 venv 的 28 项 wheel 测试、仓库外独立运行的 `python_cpu_recipe.py`、Rust SDK/Project 测试、Clippy 和 `check_coverage.py --require-complete` 均通过。Rust 与 Python 基础 fixture 使用同一 PNG、画布和矩形输入，验证相同的参数中点采样。当前验收环境为 macOS arm64；GPU 图像与 S5 agent 流程尚未运行。

S4 首批 GPU 实测：`cargo test -p kasane-sdk-observe` 的 2 项测试在当前 macOS arm64 GPU 上通过，含未保存 Session 观察及同一 Observer 的连续几何、纹理、view 变化；`cargo clippy` 无警告。带 `observe` feature 的仓库外 wheel 的 `test_observe.py` 2 项通过，覆盖 RGBA/PNG、纹理 revision、资源缺失错误、逐样本运行报告、失败报告、focus crop、contact sheet，以及 mask/Offscreen 场景；裁剪 PNG 的像素逐行等于完整合成帧相同区域。报告声明 `RGBA8Unorm`、线性数据、不额外转换的预乘 alpha 和透明背景。默认 CPU wheel 重新构建后的 28 项测试通过，且能力探测报告 GPU 观察关闭；覆盖检查 115/116、0 待绑定。完整 S4 门禁尚未执行。

观察 recipe 实测：仓库外安装的 feature wheel 运行 `examples/sdk/python_observe_recipe.py`，在 `target/sdk-acceptance/5c7e2ae6ac784b1eb80cfc4c4790a269` 产出 3 帧、3 个 focus crop、contact sheet 和报告；三帧 PNG hash 不同，adapter 为 Apple M4 / Metal，报告状态 `frames_complete`，参数 sweep 后 Session preview 值不变。此本地证据目录不纳入 Git；需要复核时重新运行 recipe 即可。

观察来源续测：增加 `input_sha256`，涵盖求值帧、验证后的纹理 hash 与输出 view；`test_observe.py` 验证几何、纹理、view 修改后的输入指纹变化与相同输入下的稳定性。报告补充 SDK 版本、原生二进制 hash、平台及明确的 session/generation/document revision。仓库外 feature wheel 的 2 项测试、Rust GPU 2 项测试及 Clippy 重新通过；默认 CPU wheel 的 28 项测试再次通过。`target/sdk-acceptance/10f9a2b6b5f74a5b8655105c3f225de5` 的三帧图像及输入指纹均不同，报告状态 `frames_complete`。

统一 wheel 门禁首跑：`python3.14 tools/validate_sdk.py --wheel <feature-wheel> --python <CPython-3.14> --require-gpu` 的报告位于 `target/sdk-acceptance/bd63cbfbfb8c4dca8c0b13c66b389278/report.json`，状态 `passed`，CPU 28 项、GPU 2 项、3 帧和 3 个 crop 均通过，adapter 为 Apple M4/Metal。CPU wheel 单独运行生成 `target/sdk-acceptance/6b201df9bcff4beaa44aa8679364d381/report.json`，状态 `partial`，GPU 标为 `not_run`。两次均使用仓库外新建的临时 venv；日志、wheel hash、源码 revision 与输入 hash 写入各自目录。此门禁覆盖 S3/S4 wheel 功能，尚不代表 S5 完整 agent 创作验收。

S5 新建流程首跑：`examples/sdk/python_two_asset_recipe.py` 在外部 wheel 中使用 2×2 与 4×4 本地 PNG 建两个不同位置的 mesh、part、Rotation→Warp 和参数 keyform；0/0.5/1 的中点插值最大误差为 `7.62939453125e-05` 原像素，保存重开采样相同，MOC3 导出 hash 为 `026dd45b531dc3bc1887dc2cec0734097373017fb522489c74a8bffd31830563`。CPU wheel 门禁见 `target/sdk-acceptance/1ceb012fb3e9455ab72317004faf6aa0/report.json`，GPU feature wheel 门禁见 `target/sdk-acceptance/8d625c2f58654c80bae20e41d397da20/report.json`；两者的 `creation_export` 均为 `passed`。外部导入编辑与二次脚本修正仍待验证。

S5 三流程续测：更新两素材模型的变形局部坐标后，两个 mesh 均在 GPU 帧中可见。`examples/sdk/python_import_edit_recipe.py` 导入 `external_v50` model3，记录原 mesh/binding/asset/parameter/deformer/part ID；修改原 mesh 的几何、deformer 父级、绑定和绘制属性，保存前后工程并导出 MOC3；未修改的资源、参数、deformer 和 part 字段在重开后保持一致，前后导出 hash 不同。`python_agent_draft.py` 生成作品、ID 清单与观察报告；`python_agent_repair.py` 实际读取原图及 focus crop 的 RGBA，诊断右侧 mesh 可见宽度仅 15 像素，低于 30 像素门槛，随后修改已有 warp 控制点并重开复测，宽度达到 45 像素、非透明像素从 165 增至 1476，另一 mesh 和资源 hash 保持不变。第二脚本若未检测到问题会失败退出，不会无条件标为成功。

最新仓库外 wheel 门禁：GPU feature wheel 的 `target/sdk-acceptance/9e70584df0f7481fb6d5c495b908fc48/report.json` 为 `passed`，CPU 28 项、S5 新建/导出、外部导入编辑、GPU 2 项及二次脚本修正均通过；CPU wheel 的 `target/sdk-acceptance/15279241181b4e1b89178e8ace857e2b/report.json` 为 `partial`，GPU 与二次脚本修正明确 `not_run`。完整 S5 还需官方运行时和图像真值对照；本报告中的 `passed` 只代表已执行的当前门禁。

二次脚本负控制：将 handoff 中的观察报告改为修正后的图像再次运行 `python_agent_repair.py`，进程以非零状态退出并报告 `No undersized target detected; existing model was not edited`，没有生成“成功修正”报告。

双 Core 数值对照：`target/sdk-acceptance/fbccc3fd74454b3389433de6e6c3ad08/report.json` 记录仓库外 GPU feature wheel、Apple M4/Metal、官方 Live2D Cubism Core 6.0.1 与 Purism Core 1.1.0。两素材导出的 3 档参数共 48 坐标，对 SDK 求值的官方 Core 最大误差 `5.960464477539062e-07` 原像素、Purism 为 0；外部模型编辑前后两档参数共 32 坐标，官方最大误差 `1.1920928955078125e-05` 原像素、Purism 为 0。`official-core-comparison.json`、`purism-core-comparison.json` 及对应 import comparison 文件保留逐项 expected/actual；源 MOC3、probe 与 wheel hash 均在报告中。此轮所有已执行检查为 `passed`；独立 GPU 图像真值对照和其它平台 wheel 分发未验收。

完整本机门禁：`target/sdk-acceptance/d146304c7c3c4190b274224ee7da0a5f/report.json` 的 10 项检查均为 `passed`，包括 CPU 28 项、S5 三条流程、真实 GPU 2 项、官方/Purism Core 的新建与导入编辑数值对照，以及独立 Godot GPU 图像参考。对 `external_v50` 默认姿态，SDK 观察 PNG 与 wgpu 参考 PNG hash 相同；相同 256×256 view 下直接比对 Godot 图，整图和 mesh crop 的 MAE、超阈值像素占比与最大通道误差均为 0。`sdk-image-comparison/comparison.json` 记录阈值、实际误差、参考/实际 hash，旁边保留原图和差分 PNG。比较器负控制将 20×20 区域像素改动后返回非零状态。最终 Rust SDK/observe/Python crate 测试、Clippy `-D warnings` 与 API coverage `115/116`、0 待绑定均通过。此结果适用于当前 CPython 3.14/macOS arm64、Apple M4/Metal 与所记录的外部 v5 fixture；其它环境需要重新运行同样门禁。

`--full` 命令另在 `target/sdk-acceptance/aefbfe59894c43a59142d5975ceaab18/report.json` 实际运行：profile 为 `full`，10 项均 `passed`。该 profile 将 GPU、双 Core 和 Godot 参考均设为必需项。
