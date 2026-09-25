# 真实大型 Live2D 模型工程压测（2026-09-25）

## 输入与转换

在本机 `~/Downloads/live2d-models` 中按完整 `model3.json` 引用的 MOC3 文件大小挑选模型。最大的 `elf01`（MOC3 30.56 MB）在导入时被 `INVALID_MASK` 拒绝；`Maid Rabbit`（MOC3 16.63 MB、贴图约 100 MB）被 `RELATION_CYCLE` 拒绝。两者没有作为成功转换的压测工程。

最终使用 `Gothic-style Girl/Gothic-style Girl/Gothic-style Girl.model3.json`：MOC3 15,588,992 字节，5 张 PNG 合计 16,849,068 字节。用 `import_model3_project` 经 `DocumentSession::import_model3_authoring` 导入，保存为自包含工程，随后重开。工程含 418 个网格、156 个普通绑定、5 个素材；清单 175,799,586 字节。导入警告和资源诊断均为 0，保存前后 `same_content` 一致。`Physics`、`DisplayInfo` 和顶层 `Groups` 属于 3 项未导入附件；本次验证的是可编辑 Document 的持久化，没有验证这些附件或重新导出 MOC3 的运行时等价性。

转换过程中发现 `serde_json` 默认浮点解析会把一个 `f64` 坐标 `0.9545455574989319` 读为相邻值 `0.954545557498932`，导致大模型初次保存重开后严格内容不等。已在工作区启用 `float_roundtrip` 并新增回归测试；修复后重新转换、重开和压测均通过。

交付工程位于本机 `output/project-io-stress/gothic-style-girl/`（被 Git 忽略，不复制第三方素材进仓库）。从压测目录复制后校验了清单 SHA-256，并再次执行了打开、保存、重开的内容相等检查。

## 测量结果

Apple M4、16 GB 内存、macOS 27.0.0、本机 APFS，Release 构建。热文件系统缓存，`DocumentSession` 原生读写路径；不含应用启动、Godot 界面刷新或网络盘。完整路径包括资源校验、结构验证和保存落盘同步。转换耗时是一次独立运行；下表多次测量为 5 次中位数，重复保存排除首次创建目标的样本。

| 指标 | 结果 |
| --- | ---: |
| 导入 MOC3 / model3 | 293 ms |
| 初次保存转换后的工程 | 1,072 ms |
| 清单大小 | 175.80 MB |
| 素材大小 | 16.85 MB |
| `decode_project` | 269 ms |
| `encode_project` | 197 ms |
| 完整打开 | 805 ms |
| 重复保存 | 1,708 ms |
| 压测进程峰值内存 | 1,261 MB |

`decode_project` 包括 JSON 校验、反序列化和 Document 构建，不等同于纯 JSON 解析器耗时。压测后重新打开保存结果，资源诊断为空，文档内容相等。

修复前，只把同一清单改成紧凑 JSON，文件降至 43,032,316 字节，完整打开的 3 次中位数为 439 ms；输入语义与保存后重开的 Document 一致。当时保存器仍输出排版 JSON，因此这个对照没有测得紧凑格式的保存收益。原排版清单经 Zstandard level 3 压缩为 11,327,979 字节；这是体积参考，未测压缩格式的完整读写路径。MOC3 是运行时格式，与可编辑工程内容不同，不能用两者的文件大小直接推算二进制工程格式收益。

## 判断与复现

真实大型工程的约 1.7 秒重复保存和约 1.26 GB 进程峰值值得优化。修复前的保存路径还有两次编码、图片完整解码和文档复制；紧凑 JSON 单独让打开时间降低约 45%。以下记录这些问题修复后的同模型测量。

## 四项修复后复测

使用网格运行时 ID 索引、去掉保存前的多余编码、按内容哈希缓存通过解码验证的 PNG 尺寸（每次仍重新读取和核对 SHA-256）、让文档克隆共享已保存基线，并改为输出紧凑 JSON。新清单仍是版本 4 的 JSON，旧排版清单仍可读取。

同一台机器、同一模型、Release 构建。旧值来自上表，修复后先读取旧排版工程并保存为新工程，再用新工程重复测试。时间为 5 次中位数；重复保存排除首次创建目标的样本。内存为整个压测进程的 `maximum resident set size`，包括解码、打开、编码、保存和重开；两个版本采用相同测量流程。

| 指标 | 修复前 | 修复后 |
| --- | ---: | ---: |
| 清单大小 | 175.80 MB | 43.03 MB |
| `encode_project` | 197 ms | 73 ms |
| 完整打开 | 805 ms | 290 ms |
| 重复保存 | 1,708 ms | 427 ms |
| 压测进程最大常驻内存 | 1,261 MB | 725 MB |

从原始 model3 再次导入并直接保存优化后的工程：导入 267 ms，初次保存 264 ms；输出到 `output/project-io-stress/gothic-style-girl-optimized/`，5 张素材共 16,849,068 字节，清单 43,034,136 字节。无导入警告和资源诊断，重开后 `same_content` 一致；清单 SHA-256 与旧工程经新保存器保存后的结果相同。原始工程保留作对照。

目前没有仅为打开/保存速度而立即迁移二进制工程格式的依据。紧凑 JSON 仍可压缩得更小，且 725 MB 进程峰值仍需在更大模型和低内存设备上观察；若要迁移格式，应再用同一语义内容测二进制原型的完整读写、兼容性和峰值内存。

```sh
cargo build --release -p kasane-project --example import_model3_project --example project_io_stress
target/release/examples/import_model3_project "$HOME/Downloads/live2d-models/Gothic-style Girl/Gothic-style Girl/Gothic-style Girl.model3.json" "$PWD/target/project-io-stress/repro-real"
target/release/examples/project_io_stress "$PWD/target/project-io-stress/repro-real" "$PWD/target/project-io-stress/repro-real-save" 5
```

原始逐次耗时在 `target/project-io-stress/real-gothic-fixed-bench.json`，一次转换的数据在 `target/project-io-stress/real-gothic-fixed-import.json`。`cargo test --release -p kasane-project` 共 40 项、`cargo test --release -p kasane-sdk` 共 42 项，均通过。

修复后逐次耗时在 `target/project-io-stress/after-fix-real-bench.json` 和 `target/project-io-stress/after-fix-compact-bench.json`，重新导入数据在 `target/project-io-stress/after-fix-import.json`。修复后 `cargo test --release -p kasane-core -p kasane-project -p kasane-sdk` 全部通过。
