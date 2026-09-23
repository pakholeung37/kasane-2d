# SDK 阶段验收记录

本文件记录已经实际运行的 SDK 验收；S3 CPU 验收已完成，完整 agent 创作 SDK 仍须完成 [实施计划](SDK-IMPLEMENTATION-PLAN.md) 中的 S4–S5。

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
