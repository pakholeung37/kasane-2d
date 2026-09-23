# SDK 阶段验收记录

本文件记录已经实际运行的 SDK 验收；完整 agent 创作 SDK 仍须完成 [实施计划](SDK-IMPLEMENTATION-PLAN.md) 中的 S3–S5。

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
