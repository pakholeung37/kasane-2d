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

导出沿用 `kasane-project` 的结构验证和 publication 机制。测试确认生成 MOC3 文件，未将 structural pass 解释为官方运行时验收。独立外部模型预检查入口、Python wheel、GPU 观察和 S5 的图像证据尚未验收。
