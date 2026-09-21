# M3C 代码复审与修正

日期：2026-09-21。审查范围：`e9779df^..c2cf7ca`（包含 e9779df），依据 M3C 实施计划检查数据契约、求值、MOC3 / 工程读写、渲染及验收工具。以下修正在工作区，未创建提交。

## 已确认的问题与修正

### P1：静态 Offscreen 导出丢失外观，求值忽略显式键映射

位置：`modules/kasane-moc3/src/encoder.rs` 的 `part_key_src.key_idx` 写入、`modules/kasane-core/src/evaluation.rs` 的静态 Offscreen 求值。

没有 Part SceneBinding、映射为空但有静态关键形态时，预览使用第一个关键形态，encoder 却写入 -1。新增测试在修正前复现透明度从 0.3 变为 1.0，颜色同样会丢失。静态求值还直接使用 keyforms[0]，忽略显式映射 [1] 或 [-1]。

修正：无绑定且无显式映射时，导出第一个静态关键形态；有映射时求值遵守映射和负哨兵。新增外观往返、显式映射及哨兵测试。

### P1：新增 BlendShape 引用依赖可选 Core 校验，非法输入可 panic

位置：`modules/kasane-moc3/src/decoder.rs` 的 BlendShape 目标表及绑定窗口读取。

Glue / Offscreen 等目标索引直接下标访问 Vec。没有可选 native consistency checker 时，section 范围合法并不意味着内部索引合法；越界目标导致 panic，异常绑定窗口还可触发过量遍历。Offscreen 关键形态和颜色索引也可能读到其他 section 的字节。

修正：对 BlendShape 目标、键表、约束引用使用 checked lookup，绑定、约束索引和关键形态窗口检查总长度；Offscreen 关键形态和颜色池按各自 count 检查。新增直接调用 decoder 的测试，绕过可选 Core 检查验证 Glue 目标、绑定偏移/长度、键表及约束/关键形态窗口越界均返回错误。此测试不等同于无 Core 构建的全量验收。

### P2：Offscreen 映射不变量没有覆盖编辑路径

位置：`modules/kasane-core/src/document.rs` 的 `validate_offscreen`、SceneBinding 创建/替换及引用删除保护。

原实现仅在 Part 已有绑定时校验索引。静态 Part 的越界索引可以写入；替换绑定、改变关键形态数量或删除绑定后，已有 Offscreen 映射会失效。保存工程后重新加载可能被新的长度检查拒绝。

修正：静态 Part 按一个关键形态槽校验，所有映射均检查索引范围；绑定创建/替换检查依赖映射长度，已映射绑定禁止直接删除或换目标。失败保持 revision 和原数据不变。新增测试先复现原有缺陷，再验证原子拒绝。

### P2：有限循环参数输入仍可计算出 NaN

位置：`modules/kasane-core/src/evaluation.rs` 的 repeat 归一化。

`(requested - minimum) / range_length` 在减法或除法阶段可能溢出，即使三个值和范围长度都有限。随后 `inf - floor(inf)` 得到 NaN，原有上下界比较无法拦截。

修正：保留普通输入的原有 f32 运算；仅在归一化溢出时使用 f64 余数计算。覆盖极大正负输入、极小周期及减法溢出，结果保持有限且在半开区间内。

### P2：验收报告把必需项目未运行或前序失败报告为成功

位置：`tools/validate_m3c.py` 的 S2 报告与 `main --stage all`。

S2 同时写入 `real_42_asset_acceptance: not_run` 和 `passed: true`，退出码为 0，与计划门禁冲突。all 模式未累计 S0–S5 的失败，在后续阶段和已有 S7 报告通过时可返回 0。

修正：S2 根据必需项计算状态，缺真实资产为 not_run / 非零退出；all 累计前序未通过阶段。新增两个 Python 测试，其中 all 测试逐一覆盖 S0–S5。实施计划同步改正 S2 状态、过期阶段摘要和“入口尚不存在”的描述。

## 实现改进建议

- 把 Part 关键形态及 Offscreen 映射提供为联合编辑 API。目前修正以拒绝非法中间态保护数据；编辑器若需要增加或删除关键形态，应一次提交两者，避免调用者先清映射再重建。
- 给 Offscreen 静态/绑定键选择建立共用规则或小型接口，供 evaluator 和 encoder 共同使用；本次数据丢失来自两处各自推断默认值。
- 把 `document_preview.rs` 中资源预算、mask 资源、Offscreen surface 生命周期和渲染命令执行拆分为模块。当前一个长方法同时修改多类 GPU 资源，使前置失败和清理路径难以独立验证。
- 验收工具应以每项实际运行结果生成 gate，减少硬编码 `True`；区分构造 fixture、真实外部模型、数值参考、GPU 参考和独立打包证据。S3 / S4 的构造模型不能自动证明真实资产覆盖。
- 版本 section 数量和能力访问应集中在 schema/layout；decoder 仍大量使用数字 section 下标，增加新版本时容易访问错误字段。无 Core 构建还应增加独立的畸形输入测试任务，不能用“链接 Core 时测试通过”替代。

## 验证及限制

- `cargo test -p kasane-core -p kasane-moc3 -p kasane-project -p kasane-godot --locked`：117 项通过，其中新增 6 项 Rust 回归测试。
- `cargo check --workspace --locked`：通过。
- `python3 -m unittest discover -s tools -p test_validate_m3c.py`：2 项通过。
- `git diff --check`：通过。
- 本轮未重跑官方 Framework GPU 全矩阵、无 Core 独立构建或 S7 打包验收；未修改 shader。既有 S6 报告不能当作本轮重新运行的证据。真实 4.2 资产及 S7 仍是总体完成的缺口。
