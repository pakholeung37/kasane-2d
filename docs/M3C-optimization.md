# M3C Review 建议实施记录

本轮实现针对 Review 中的五项建议，保留已有 review 修复。未改变工程文件格式。代码未提交。

## 1. Part / Offscreen 联合编辑

新增 Rust 与 Godot 同名入口：
`replace_part_binding_with_offscreen(binding, offscreen)`。

传入完整候选 SceneBinding 与 Offscreen，包括参数轴、Part 关键形态、Offscreen 外观及映射。接口要求二者已经存在，且保持原 Part 所有者；先在候选 Document 上校验全部状态，再发布一次修改。失败不改变内容或 revision；成功仅增加一次 revision。

新增槽位的外观由调用者明确指定。映射采用参数轴的规范槽位顺序；绑定输入经规范化后，映射仍应对应规范顺序。接口不猜测插值、复制或中性值策略。既有单对象替换接口继续拒绝破坏另一方映射的写入。

Editor 使用已有动作机制形成一次撤销记录：

```gdscript
var started = workspace.begin_action("Insert Part keyform")
if not started.ok:
    return started
var result = workspace.document.replace_part_binding_with_offscreen(binding, offscreen)
workspace.end_action()
return result
```

该 API 是编辑能力，不额外增加关键形态面板。Rust 测试覆盖拒绝原子性、单次 revision、快照恢复、工程保存重开；Editor 脚本覆盖实际动作的 undo / redo，并将修改后的 Ren 导出后与官方 GPU 比较。

## 2. 共享键选择与连续导出

`Offscreen::keyform_index` 是 evaluator 与 encoder 的共同规则：

- 无 Part 绑定、空映射、有静态外观：选择首个外观。
- 有绑定、空映射：中性外观。
- 显式负哨兵：中性外观。
- 正常索引：选择引用外观。
- 映射长度、槽位或索引越界：返回错误。

联合编辑往返测试发现了额外问题：MOC3 运行时以首个 Part key 对应的 Offscreen key 为基址，按插值槽位访问连续窗口。直接导出文档内的任意索引映射不能可靠表达插入、重排、复用或负哨兵。

现在导出按 Part 槽位物化连续 Offscreen 外观；重复引用复制相应值，负哨兵写为 opacity=1、multiply=[1,1,1]、screen=[0,0,0]。乘色与屏色池也连续对齐。工程内仍保留原映射，MOC3 往返保证求值外观，不保证局部关键形态的存储索引相同。

回归覆盖插入后 `[0,1,-1]`、重排 `[-1,1,0]`、复用 `[1,0,1]`、全中性映射，在关键值及中点比较透明度、乘色和屏色。

## 3. 预览器分阶段

将 `document_preview.rs` 中职责抽为内部模块：

| 模块 | 职责 |
|---|---|
| `resources.rs` | 帧和纹理预检、活动 Surface 集合、尺寸与共享内存预算 |
| `surfaces.rs` | Surface 创建、复用、调整尺寸与过期资源清理 |
| `masks.rs` | Mesh 与 Offscreen 共用遮罩更新和生命周期 |
| `commands.rs` | 按 render_plan 重父级、排序、放置目标颜色拷贝 |

资源变更前生成 SubmissionPlan；常见非法输入和超预算在该阶段返回。Mesh 与 Offscreen 的 mask scale 差异显式传入，共享实现，避免两套代码漂移。保持渲染顺序和 shader 算法，通过 GPU 生命周期、混合矩阵和真实模型对照验证。

这是职责与失败边界的整理，不承诺驱动级分配失败可以事务回滚，也不把代码拆分当作参数拖动性能改善。

## 4. 验收由证据计算

新增 `tools/acceptance_evidence.py`。每项记录 id、required、status、reason 以及证据路径和 SHA-256。

- 必需项失败：failed。
- 必需项未执行，或声称通过但没有证据：not_run。
- 全部必需项有完整证据且通过：passed。
- 证据缺失或被修改：failed。
- 缓存报告仅在源码 revision、工作区内容指纹（含场景、资源和构建配置）、子模块状态及证据 hash 匹配时可复用。

S0–S5 保存 cargo 原始日志与非零测试数量、双 Core 探针输入/输出及模型、探针哈希；S6 保存每个 gate 的结果快照和引用的图像/报告哈希。执行前覆盖旧报告，异常写入失败报告，防止旧 passed 留存。S0 的历史断言不再当作本次复现；缺少真实外部样本或 Editor/GPU 证据时保留 not_run。

构造 fixture、真实外部资产、数值参考和 GPU 参考分别记录。当前改进没有补齐所有版本的真实外部素材，也不代表 S7 独立打包已重新验收。

## 5. 版本布局与无 Core 校验

schema 集中声明 167 个命名 section、元素宽度、关联 count、最小版本；VersionLayout 统一版本对应的 offset/count 表、section 数和 loader reserve。decoder 使用命名 section，encoder 请求不支持的字段会报错。

新增 Rust-only 安全层，在可选原生校验之前检查表引用、窗口、几何池跨度、变形器类型及局部索引、Warp 网格与顶点数量等。decoder 自己从输入重建结构检查，不能被外部传入的过期 InspectionReport 绕过。

无 Core 测试命令：

```sh
KASANE_MOC3_DISABLE_CORE_VALIDATION=1 cargo test -p kasane-moc3 \
  --no-default-features --test safety_tests --locked
```

同时关闭 feature 和构建开关：测试依赖中的 feature unification 可能重新启用默认 feature，因此环境开关用于确保不链接 Core；测试对编译后的 HAS_CORE_VALIDATION 再次断言。CI 已增加独立步骤。

六项测试覆盖未知版本、截断、负数/超大索引、跨表窗口、v6 引用和过期报告，并对三个 fixture 的非空四字节 section 做定向变异。测试曾暴露巨大 Warp 行列乘法溢出，现以 i64 校验网格与顶点数，先拒绝再解码。额外拒绝旧版本伪造的新版本计数，并在分配前校验绑定的笛卡尔积与实际关键形态窗口；回归用小文件构造了 8^16 个组合，确认返回错误而非尝试巨量分配。这是有范围的边界回归，不是完整模糊测试或所有损坏文件的安全证明。

## 验证结果

本机 Apple M4 / Godot 4.7.2 mono，GL Compatibility。报告位于 `target/kasane/m3c/`（忽略的本地产物）。

| 验证 | 结果 / 证据 |
|---|---|
| 四个 Rust 包 | 125 项通过；`optimization-evidence/s5/` 下各包日志 |
| 真正关闭 Core | 6 项通过；`optimization-evidence/m3c-no-core.log` |
| 验收脚本负控 | 12 项通过；`optimization-evidence/m3c-python.log` |
| S5 / Ren 双 Core | passed；`optimization-evidence/s5_report.json`。三组采样，位置最大误差 2.6226043701171875e-6，Offscreen 透明度误差 0 |
| S2 缺失证据行为 | not_run；`optimization-evidence/s2_report.json`。构造 fixture 和 Rust 测试通过，真实外部素材及对应 Editor/GPU 仍缺失 |
| S6 全量最终代码验收 | passed，9/9 gate；`optimization-final-s6/report.json`。147 个 Mao 数值用例分别通过官方/Purism Core 对照，Editor 的 10 组官方 GPU 比较全部通过（含联合编辑后 3 个采样），源码及证据 hash 校验一致 |
| Release 构建与源码 Editor 启动 | 通过；release 库已原子替换至 `apps/editor/native/libkasane_godot.dylib`，启动日志为 `optimization-evidence/m3c-source-startup.log` |

`cargo fmt --check` 仍在未改动的九个既有文件中报告格式差异；严格 Clippy 在已有 evaluator 的六处 `needless_range_loop` 处失败。本轮没有顺带重排整个仓库。原始日志：`optimization-evidence/m3c-final-fmt.log`、`optimization-evidence/m3c-final-clippy.log`。因此不宣称全仓 CI 已通过。

参数卡顿的已测结论及 release/debug 对照见 [M3C-parameter-performance.md](M3C-parameter-performance.md)。本轮五项改进未新增整帧缓存或改变参数事件链。
