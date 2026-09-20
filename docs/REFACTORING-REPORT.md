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

# 验证器负控制门禁
python3 -m unittest discover -s tools -p test_acceptance_validators.py -v

# 官方 Cubism Core 与 Purism Core 对照
python3 tools/validate_official_core.py

# 真实 GPU 视觉回归测试
python3 tools/validate_gpu.py --library target/release/libkasane_godot.dylib

# Godot 边界、生命周期与完整工作流
python3 tools/validate_godot.py --library target/release/libkasane_godot.dylib --suite all

# 生产级规模性能基准
cargo bench -p kasane-core --bench production_scale_benchmark
```

本轮本机结果：

- 工作区 30 个测试通过，其中 project 22 个；包含真实文件操作、并发及故障注入。
- Clippy、rustfmt 和 Release 构建通过。
- 使用新构建 Rust 动态库的 Godot 无头边界检查：59/59 通过。
- 双向迁移检查：12 项通过，包括 C++→Rust→C++→Rust，以及 C++ 持锁时 Rust 保存被拒绝。
- 双向 roundtrip 后，C++ 工具独立输出的源 JSON、求值采样、MOC3 和 model3 JSON 与迁移前逐字节一致。

本地报告位于 `target/rust-migration/report.json` 和 `target/rust-fixes/godot-boundary/report.json`；目录不提交。持续门禁位于 `.github/workflows/rust.yml`。原有 C++ 门禁保留。

## 完整替代验收进展（尚未全部达标）

针对此前尚未建立的结论，本阶段已增加部分验收测试（详见 [`docs/COMPATIBILITY-CONTRACT.md`](COMPATIBILITY-CONTRACT.md) 与测试报告）：

1. **官方 Cubism Core 对照**：
   - 链接官方 `Live2DCubismCore` v6.0.1 探针（`tools/validate_official_core.py`）；
   - 在 1D、2D 参数、嵌套变形与遮罩用例的 28 个参数采样点下，Rust 求值结果与官方 Core 的最大顶点坐标误差约 0.05348 像素（按逐坐标绝对/相对容差判断；未统计平均误差），图层排序、透明度、乘色/滤色和遮罩关系 100% 一致。
2. **真实 GPU 视觉回归**：
   - 在 Apple M4 GPU（Metal 兼容渲染器）上，以官方 Core 驱动的 `GDCubismUserModel` 为基准；
   - 检验 Rust `KasaneDocumentPreview` 的混合模式、正反向剪贴遮罩、多层嵌套变形、透明羽化及动态纹理热更；
   - 84 项 GPU 检查全部通过（全图均值差异 0.0，关键特征单通道最大误差 0.001688 $\ll 2/255$）。
3. **Godot 生命周期安全与完整工作流**：
   - 句柄世代失效保护（`STALE_HANDLE`）、多预览销毁隔离、信号回调重入借用安全、50 轮连续加载卸载压力测试全部通过（修正后 128 项 Godot 检查通过）；
   - 自动化闭环测试验证：新建工程 → 导入素材 → 构建拓扑与网格 → 配置参数形态 → 实时预览 → 保存落盘 → 干净重开 → 导出包交给官方 Core 完成一致性校验、加载和三个参数值的求值对照，并检查 model3 引用的纹理文件存在。重开目前仍在同一进程。
4. **生产级真实规模性能基准**：
   - 建立 80 个网格、8,000 顶点、12,960 三角面、5 层嵌套变形及 8 个参数（仅 2 个参数驱动网格绑定，变形器为静态）的生产级模型；
   - 无帧间等待的核心求值采样（连续 3,600 帧）：平均 433.95 µs/帧（p50 426.83 µs, p99 580.21 µs），吞吐率达 **2,303.5 FPS**（较 16.6ms 帧预算充裕 28.7 倍），顶点吞吐率 18.43 M verts/s；
   - 交互式拖拽求值（连续 1,000 次）：平均 447.71 µs/次（p50 426.79 µs, p99 1.00 ms），吞吐率达 **2,233.6 updates/s**。

当前不能宣告满足退役 C++ 的全部条件。尚需补齐：

- C++/Rust 操作序列差分（当前随机测试仅验证 Rust 不变量与确定合法操作的成功结果）。
- 独立进程退出后重开、目标平台打包和干净环境安装验收。
- 真实进程中断恢复与长期 RSS 收敛验证；对象计数检查不能替代 RSS。
- 官方 Core、Godot 与 GPU 验证的持续运行环境；当前 Rust CI 未运行这三个入口。

上述基准仅测核心编辑/求值耗时，不包含 Godot、GPU 或完整应用帧时，也不构成 8 轴动画或内存门禁。


## 验收测试审查修正

- 官方比较器严格检查数组长度、点维度、颜色分量和有限数值；空样本、缺失夹具失败，失败执行不保留旧通过报告。
- Godot 工作流比较保存前后求值结果，并使用官方 Core 验证本次导出包。
- 随机序列为确定合法的编辑断言成功、内容变更及 revision 推进；仍不是 C++ 差分。
- `python3 -m unittest discover -s tools -p test_acceptance_validators.py -v` 提供 8 项比较器回归测试，含空数组、截断数组、NaN/Inf、错误坐标和 8 字节 MOC3。纯 Python 用例纳入 CI；真实官方探针用例在未构建 SDK 探针的环境显式跳过。
