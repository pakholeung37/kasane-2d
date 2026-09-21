# M3B 实施审查与修复记录

日期：2026-09-21。审查范围：`1b8774d`（含）至 `ced11a5`，即 Stage A–F 六个提交。修复位于当前工作区，未自动提交。

## 1. 结论

Mao 的静态 Glue、BlendShape 及 MOC3/工程往返已实现，但**尚未达到 [M3B 实施文档](M3B-mao-editable-import.md) 的完整可编辑与验收要求**。Rust 库测试通过不代表正式 Editor 的新增语义可以编辑，也不代表 GPU 一致性或完整模型包脱离验收通过。

本次修复了数据丢失防护、颜色语义、绘制顺序、修改校验和验收误报，并把 Mao 参数采样从 17 组扩展至 550 组。扩大采样后仍有严格数值门禁失败，不能宣布 M3B 完成。

## 2. 已修复的问题

| 优先级 | 问题与触发条件 | 修复及证据 |
|---|---|---|
| P1 | 动画 Glue 导入读取 `_binding_idx` 后丢弃，永远使用首个 intensity；`Glue.binding_id` 又错误地允许引用 MeshBinding，而求值并不读取它 | inspector/decoder 明确拒绝动画 Glue；Document 拒绝伪造的 binding 引用。增加合法三关键值 Glue 文件的拒绝回归。**仅消除静默丢失，完整动画 Glue 仍待实现** |
| P1 | BlendShape 颜色一端为 `None`、另一端有值时，Rust 将有值的一端加入；encoder 将 `None` 写成零颜色，丢失 `-1` 缺省语义 | 插值两端任一缺省时跳过该绑定的对应颜色贡献；写出保留负偏移。回归覆盖端点、中点、A→B→A、重导入字段和 PurismCore 数值 |
| P1 | 缩小 BlendShape 参数范围、改变类型时未检查 key table/constraint；只受 BlendShape 驱动的 Warp 可改变网格/类型，留下错误长度或目标类型 | 修改前在候选数据中检查新依赖；禁止未更新形态的网格/类型替换。回归验证失败后 Document 内容不变 |
| P1 | Stage F 将有限数值测试的成功报告为整个里程碑通过；Stage A 对压力用例单独放宽至 0.07 像素，且使用固定 `2e-5` 代替逐值混合容差 | 恢复统一 0.05 像素和逐值容差；GPU、打包应用、纹理脱离、编辑往返明确 `not_run`。仅显式 `--numerical-only` 允许数值子集成功退出，完整门禁不会假通过 |
| P2 | Mesh/Part 普通 draw order 未先转整数就与增量相加；Part 额外重复加 epsilon，边界结果不同 | 普通形态先转换，增量之后按 Core 顺序 `+0.001`、钳制、转整数；小模型小数顺序用例与 PurismCore 对照 |
| P2 | `inspect_moc3_safety` 遇到范围外特性会跳过 Core 一致性校验，损坏文件可返回安全报告；count 相加还可能发生 i32 溢出 | 范围外文件同样执行一致性校验，计数和扩大到 i64；新增“循环参数 + 非法 Mesh 父索引”回归 |
| P2 | Glue runtime ID 写入器没有长度/编码检查，超过 64 字节被 `resize` 截断 | `write_id` 检查 1–63 字节可打印 ASCII，失败定位字段；增加 64 字节 ID 回归 |
| P2 | constraint 被不必要地限制只能引用 BlendShape 参数；普通 binding 又可错误引用 BlendShape 参数，导出后 Core 不按普通表处理 | constraint 允许引用 Normal 参数；普通 Mesh/Scene binding 要求 Normal 参数，修改类型也检查依赖 |
| P2 | 未知 MOC3 参数类型被当作 BlendShape；GDScript 中未知 kind 被当作 Normal | 明确拒绝未知类型/错误字段，仅未提供 kind 的旧脚本默认 Normal |
| P2 | 多个 BlendShape target group 或共享 binding 窗口可能被 HashMap 覆盖/合并，丢失源分组及分组间钳制语义 | 当前 Document 尚无分组表达，decoder 明确拒绝无法表示的结构；增加两个合法 group 指向同一 Mesh 的拒绝回归 |
| P2 | 删除 Warp/Rotation/Part 的 BlendShape binding 没有把后代 Mesh 放进变化集合 | 删除增量绑定时保守标记全部 Mesh，包含 Glue 传播影响 |
| P2 | 手写采样里的未知参数静默忽略，`ParamHairBack` 实际并不存在 | 未知名字直接报错，改为实际 `ParamHairBackL/R`；补全部参数端点、普通/增量键和约束键及中点，分批输出控制内存 |

注意：同一目标多个 group 的拒绝是防止错误导入的临时兼容边界，不等于完成通用分组支持。Mao 不触发该限制。

## 3. 仍需完成的阻塞项

### P1：Editor 新增语义不可通过正式脚本 API 编辑

`modules/kasane-godot/src/document_bridge.rs` 没有 BlendShape table/constraint/binding、Glue 的 CRUD、读取快照或列表接口；`get_document_summary` 也未提供这些集合。六个 stage 没有修改 `apps/editor`、对应 dock 或 `API.md`。

因此 Rust Document 中新增的数据可以被保存和求值，但 Agent 在正式 Editor 中不能按 M3B 契约检查/修改它们。Stage E 的 commit 标题虽然包括导出和脱离往返，不能据此视为 F10 或完整阶段 E 已完成。

建议下一步先补类型明确的 bridge 方法与 GDScript 数据契约、错误结果、变化信号及快照，再接检查面板；用正式应用执行至少一次各类型编辑→撤销/重做→保存→导出。不要把手改工程 JSON 当成正式编辑 API。

### P1：官方 Core 数值精度门禁仍失败

在原始 Mao、其他参数默认且 `ParamInkDrop=30` 时，本次数值报告记录：

- `ArtMesh174`，第 0 顶点 X：官方 Core `-0.48575371503829956`，Document `-0.48576971888542175`。
- 差值约 **0.092822 像素**，超过 0.05。550 组中的 `source_document` 与 `document_export` 两条官方路径在该批失败。
- 同一原文件 PurismCore 的该点为 `-0.48576274514198303`，相对官方约 **0.052375 像素**。说明差异不能全部归因于导入器；PurismCore 基线本身也需要处理。
- 该批官方 `source_export` 最大误差约 **0.000691 像素**；完整 550 组的官方 `source_export` 对照通过。
- Stage A 原有 `all_maximums` 样本的双 Core 最大位置差约 **0.063610 像素**，恢复严格阈值后正确失败。

这些是已复现数值，尚未证明具体算术根因。建议保留失败参数和局部对象，按原始 Core→PurismCore→Rust 求值分层检查嵌套变形、边界外推、f32 运算顺序与坐标转换误差。不能用提高全局阈值或删除样本作为修复。

### P1：普通 Glue 强度绑定仍未实现

Document 只有固定 intensity，没有独立的 Glue Keyform/普通多轴强度绑定。当前 `binding_id` 无法承担这一职责。本次已改成拒绝，Mao 的固定强度 Glue 仍可用；原计划 F06 的可编辑普通强度绑定尚未完成。

应新增专用 GlueBinding/GlueKeyform，贯通导入、求值、保存、脚本编辑和导出，再移除临时拒绝；用非恒定强度验证，不能仅依赖 Mao 的 7 个强度均为 1 的样本。

### P2：联合拓扑编辑仍缺少原子提交接口

`Document::replace_mesh_with_keyforms` 只接收普通 MeshBinding；`replace_mesh` 会拒绝已有增量形态的顶点 ID 变化。当前 transaction 也不能完成普通形态、增量形态、Glue 映射的联合结构修改。

安全拒绝比破坏数据好，但不等于完成 F07。需要一个候选文档提交接口，同时校验普通 Keyform、全部增量顶点数据、Glue 点对及 VertexMapping；失败保留原内容，成功只产生一次 revision/Undo 步骤。

### P1：完整应用、GPU 和素材脱离证据缺失

`export_m3b_cases` 使用裸 MOC3 + 空纹理映射，并在内存中 encode/decode 工程 JSON；它没有加载真实纹理，没有移走源素材目录，也没有启动正式 Editor。这个测试只能证明模型数据不依赖源 MOC3，不能证明整个资源包可脱离源目录。

当前 report 将 GPU 对照、打包 Editor 工作流、纹理工程脱离、全部新增编辑类型往返保留为 `not_run`。550 组单参数/既有组合采样也尚未实现文档要求的全部“共享目标驱动×约束×普通变形”依赖组合。应分别补门禁，不继续扩张一个数值测试脚本的完成声明。

## 4. 实现改进建议

1. **先补可编辑闭环，再做性能优化。** 当前最大缺口是 bridge/API、Glue 强度和联合拓扑编辑。先逐项满足 F06/F07/F10 的行为验收。
2. **保留类型与分组语义。** 不要用 MeshBinding 代理 GlueBinding；不要用 target→binding HashMap 抹掉 MOC3 的有序 group。若要支持共享/重复 group，新增明确的数据结构和分组钳制测试。
3. **集中格式映射。** decoder 大量使用裸 section 数字，而 encoder 使用 schema 字段名。建议从共同 schema 生成/定义命名索引，减少 114、125、143 等魔法数字的维护风险。
4. **错误引用不要降级到槽 0。** encoder 仍有 `unwrap_or(0)`、找不到顶点则返回 0 等兜底；目前主要由 Document 校验保证可达性，后续扩展应改为携带对象/字段的错误，防止新路径绕过校验后静默串引用。
5. **求值索引按测量结果优化。** 当前每个对象每帧扫描全部 BlendShape binding，同一 constraint 也被重复求值。先在 Mao 连续拖参测量，再考虑按 revision 构建 target 索引、每帧复用参数/constraint 结果；缓存不能改变叠加顺序或污染持久化数据。
6. **复用验收逻辑并保留失败证据。** 本次让 Stage A 复用严格比较器并重建 Core 探针，Stage F 收集各批失败而不在首个误差中止。仍建议报告按 planned requirement 编号对应具体 passed/failed/not_run，避免 commit 的“stage complete”名称取代验收事实。

## 5. 本次验证

以下路径相对仓库根目录，生成报告未纳入源码提交：

| 检查 | 结果 |
|---|---|
| `cargo test -p kasane-core -p kasane-moc3 -p kasane-project --locked` | 63 项测试通过（另有 0 项的 lib/doc test target）；含新增修改校验、颜色/排序、动画 Glue 拒绝、ID 容量、安全检查及重复 group 回归 |
| `cargo test -p kasane-godot --lib --locked` | 6 项通过 |
| `cargo check --workspace --locked` | 通过 |
| `python3 tests/test_m3b_validation.py` | 4 项通过：像素上限、逐值颜色容差、重复 ID、正常对照 |
| `python3 tools/validate_m3b.py --numerical-only --output-dir target/kasane/m3b-review` | 550 组、23 批、2 个 Core，严格门禁失败；报告 `target/kasane/m3b-review/report.json` |
| `python3 tools/m3b_baseline.py --output-dir target/kasane/m3b-review` | 严格门禁失败，`all_maximums` 超标；报告 `target/kasane/m3b-review/baseline.json` |
| GPU / 正式应用验收 | 本次未执行；实现范围仍缺上述入口和测试 |

M3B 当前状态应为：**核心导入/往返已有实现，部分缺陷已修复；完整可编辑能力与严格验收未完成。**
