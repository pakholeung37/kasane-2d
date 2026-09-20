# Rust 迁移状态与修复记录

本报告替代原 v1.0 总结。Rust 实现已经可构建，并通过本文列出的检查；原报告关于“完全兼容”“零分配”“全面提升 11–22 倍”的结论撤回。当前仍保留 C++ 实现用于兼容性对照，不等于所有应用入口与发布流程都已切换到 Rust。

## 模块与构建条件

| Crate | 职责 |
|---|---|
| `kasane-core` | Document、几何编辑、引用关系、关键形态、纯求值 |
| `kasane-moc3` | MOC3 编码与 Purism 对照测试 |
| `kasane-project` | 工程格式、资源验证、保存与运行包发布 |
| `kasane-godot` | Godot GDExtension 绑定与预览 |

工作区最低 Rust 版本为 **1.94**，由 Cargo manifest 显式声明，与 godot 0.5 的要求一致。当前本机验证使用 macOS ARM64、Rust 1.98.1。Godot 绑定选择 `api-4-3`；其他引擎版本需分别验收。

Rust 路径通过 `sha2` 和 `png` 处理哈希和图像，不需要系统 OpenSSL/libpng。但当前 `kasane-moc3/build.rs` 仍编译仓库中的 PurismCore C 源码，所以需要 C 编译器及已初始化的子模块。不能称为“零 C 工具链依赖”。运行 C++ 对照工具仍需 CMake、libpng、OpenSSL；现有 SCons/CMake 工程尚未删除。

## 本轮修复

| 审查问题 | 修复及回归证据 |
|---|---|
| 导出可能删除源工程 | 在目录移动前解析实际路径，拒绝目标包含工程根或任何源素材；覆盖相同路径、祖先路径、符号链接别名及外部替换素材 |
| Warp/Rotation 文件数值颠倒 | 恢复 directory-project v1 的 Warp=0、Rotation=1；使用已有 C++ sample 和双向工具验证 |
| 修改绑定中的变形器类型/网格导致 panic | 保留绑定时返回 `KEYFORMS_REQUIRED`，文档及 revision 不变 |
| 参数范围收缩留下非法关键形态 | 在候选文档重新验证 MeshBinding/SceneBinding，失败不提交；合法修改通知受影响网格 |
| 另存为静默覆盖其他工程 | 没有已打开基线且目标存在时返回 `DESTINATION_EXISTS` |
| 并发保存丢失更新 | 恢复 `.kasane.lock` 协议，在锁内检查基线，发布前再次检查；8 个写者只允许 1 个成功，C++ 持锁时 Rust 返回 `PROJECT_BUSY` |
| 复用损坏的同名素材 | 校验文件类型及哈希，不匹配时独占创建新名字，保留旧文件；中断后的部分文件也不会被复用 |
| 导出绕过素材哈希 | 验证所有项目素材，包括未使用素材；发布的是已验证的同一批字节，不在校验后重新读源文件 |
| 删除同步却报告 durable=true | 写入使用完整写入与文件同步，提交前同步目录，发布后同步失败返回成功、`durable=false` 和 warnings；保存基线仍推进 |
| null 原型字段导致旧版拒绝打开 | 不再输出不存在的 legacy 字段 |
| 索引色和 16-bit PNG 拒绝加载 | 增加展开和位深转换，覆盖调色板、低位深灰度、16-bit、RGB/灰度透明色 |
| 无效性能倍数 | 删除固定 C++ 时间与过期 JSON 比较，不再把这些 microbenchmark 称为跨语言性能门禁 |

另补回了运行包 `export-report.json`。包替换失败会回滚；回滚本身失败会保留旧包并在错误中返回恢复路径。文件系统提交操作可通过 `FileSystem` 注入，测试覆盖部分写入、文件同步、目录同步、提交重命名、回滚以及提交前外部改写。

`publish_package` 现在返回 `Publication`，调用者需检查 `warnings`/`durable()`；`DocumentSession` 会将其转换为原有 Godot 结果字典中的 `published`、`durable` 和 `warnings`。显式验证回调仍由调用者提供。Session 的默认回调只检查非空编码产物，不应把每次导出描述为完成了官方 Core 运行时验证。

锁只协调遵守协议的写者，不是防止任意外部程序改写文件的隔离机制。非 Unix 平台当前缺少等价目录同步，发布会返回持久化未确认的 warning；新增 CI 覆盖 Linux/macOS/Windows，但本机执行结果不代表远端 CI 已通过。

## 验证入口

```sh
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release -p kasane-godot --locked

# macOS Godot 无头边界检查；其他平台需对应工具入口。
python3 tools/validate_godot.py --library target/release/libkasane_godot.dylib

# C++/Rust 真实进程间的格式与锁协议验证。
cmake -S . -B target/migration-cpp -DCMAKE_BUILD_TYPE=Release -DKASANE_MOC3_CONFORMANCE=OFF
cmake --build target/migration-cpp --target kasane_document_tool --parallel 4
cargo build -p kasane-project --example project_roundtrip --locked
python3 tools/validate_rust_migration.py \
  --cpp-tool target/migration-cpp/modules/kasane-core/document/kasane_document_tool

# 独立 Rust 耗时采样，不代表跨语言性能比较。
cargo bench --workspace
```

本轮本机结果：

- 工作区 30 个测试通过，其中 project 22 个；包含真实文件操作、并发及故障注入。
- Clippy、rustfmt 和 Release 构建通过。
- 使用新构建 Rust 动态库的 Godot 无头边界检查：59/59 通过。
- 双向迁移检查：12 项通过，包括 C++→Rust→C++→Rust，以及 C++ 持锁时 Rust 保存被拒绝。
- 双向 roundtrip 后，C++ 工具独立输出的源 JSON、求值采样、MOC3 和 model3 JSON 与迁移前逐字节一致。

本地报告位于 `target/rust-migration/report.json` 和 `target/rust-fixes/godot-boundary/report.json`；目录不提交。持续门禁位于 `.github/workflows/rust.yml`。原有 C++ 门禁保留。

## 尚未建立的结论

- 未在本轮执行 GPU 视觉回归或官方 Cubism Core 对照；无头检查不能替代两者。
- 没有公平的跨语言性能结论。旧数字来自未优化 C++ 基线与 Rust Release，部分 fixture 也不同。重新比较必须使用同一输入、优化构建、结果等价检查及可复核的编译参数和原始样本。
- 求值仍有哈希表、字符串和 `Vec` 分配；未建立“热路径零分配”“内存减少 80%”或特定 SIMD 指令的证据。
- 尚未验收全部应用入口迁移、跨平台桌面打包和完整编辑器功能。

上述兼容性仅针对既有 `kasane-directory-project` v1。中间的错误 Rust 编码器也写了 version 1，但颠倒类型映射；没有可靠版本标记能无歧义自动识别。若留有该中间版本生成的文件，应基于备份单独修复，不通过猜测兼容逻辑改变既有 v1 的定义。
