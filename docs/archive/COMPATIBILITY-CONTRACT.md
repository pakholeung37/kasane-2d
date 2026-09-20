# Kasane 2D 业务行为与兼容性契约 (Compatibility Contract)

本文档定义 Kasane 2D 核心数据模型、Godot 绑定与 MOC3 发布的技术契约。在推进 Rust 完整替代 C++ 实现的过程中，遇到两者行为分歧时，**以此契约作为唯一判据**，不默认旧 C++ 实现无条件正确。

---

## 1. 核心模型与拓扑不变量 (Core Document Invariants)

1. **DAG 有向无环图约束**：
   - `Part` 的 `parent_id` 必须为空或指向已存在的 `Part`，严禁形成父子循环。
   - `Transform`（Warp / Rotation）的 `parent_id` 必须为空或指向已存在的 `Transform`；其 `part_id` 必须为空或指向有效 `Part`。严禁形成变形器层级循环。
   - `Mesh` 的 `deformer_id` 必须为空或指向已存在的 `Transform`；其 `part_id` 必须为空或指向有效 `Part`。
   - 任何试图引入父子循环的挂载（如 A 挂 B 且 B 挂 A、或自我挂载 A 挂 A），必须被底层原子拦截并返回错误（`RELATION_CYCLE`），绝不可损坏现有拓扑。

2. **引用完整性与级联保护**：
   - 任何正在被 `Mesh`、`MeshBinding`、`SceneBinding`、`Transform` 引用的对象（素材、参数、部件、变形器），调用 `erase_object` 时必须拒绝删除并返回 `OBJECT_REFERENCED` 与引用者列表。
   - 只有显式解绑或删除全部引用者后，方可清理目标对象。

3. **操作原子性与回滚契约**：
   - 任何失败的编辑操作（包括非法参数范围、非有限数值 `NaN`/`Inf`、格式校验失败、成环挂载等），必须保证：
     - `doc.revision()` **完全不增加**；
     - `doc.modified()` **脏状态保持原样**；
     - 文档内容（内存结构与哈希）**100% 保持在执行前状态**。
   - 事务支持：`begin_transaction()` 开启后，在执行 `commit_transaction()` 之前所有暂存修改均可被 `cancel_transaction()` **完全回滚至事务开始前的绝对相同状态**。

4. **预览隔离 (Preview Isolation)**：
   - 调用求值器 `evaluate_frame` 或通过桥接层调用 `set_preview_values`，属于纯呈现层动作。
   - 预览参数与求值过程**绝不污染**文档持久数据、**绝不推进** `revision`、**绝不改变** `modified` 保存基线。

---

## 2. 工程持久化与锁协议 (Persistence & Concurrency)

1. **格式定义**：
   - 规范工程格式为 `kasane-directory-project` version 1。
   - 工程入口文件为 `project.kasane.json`。
   - 素材统一组织在 `assets/` 目录下，常规文件名为 SHA-256 散列值 + `.png`；同名损坏文件恢复允许带唯一后缀的文件名，完整性以内容哈希为准。
2. **容错与规范化解析**：
   - `Parameter` 的 `decimal_places` 字段在缺省时必须自动安全回退为默认值 `6`，不得因缺少该字段拒绝加载合法历史工程。
   - 序列化输出不得包含任何未在模式中声明的废弃或 `null` 原型字段。
3. **并发写入保护**：
   - 持久化遵循 `.kasane.lock` 文件互斥协议。
   - 当检测到锁文件已被外部进程持有或正在写入时，写操作必须安全放弃并返回 `PROJECT_BUSY`，严禁多进程静默覆盖导致工程损坏。
4. **事务性发布 (Atomic Publication)**：
   - 运行包导出（`export_package`）采用临时暂存目录 + 完整校验 + 原子重命名机制。
   - 提交前同步失败必须中止发布；已提交后的目录同步失败返回 warnings，并标示 `durable=false`。

---

## 3. Godot GDExtension 生命周期边界安全 (Lifecycle Boundaries)

1. **句柄失效安全性 (Handle Invalidation)**：
   - 数据句柄（`KasaneMeshData` / `KasaneDeformerData`）维护 `owner_id` 与 `generation` 标号。
   - 当底层工程重新打开（`open_project`）或文档被重置时，`generation` 递增。
   - 既有句柄即刻判定为失效（`is_valid() == false`）；任何对失效句柄的调用均安全返回字典 `{"ok": false, "code": "STALE_HANDLE", ...}`，杜绝野指针访问与内存越界。
2. **多预览节点独立性**：
   - 允许多个 `KasaneDocumentPreview` 节点同时监听同一个 `KasaneDocumentBridge`。
   - 任意预览节点的销毁（`free()` 或 `queue_free()`）必须通过信号解绑与清理内部视图，不得影响底层 Document 或其它并存的预览节点。
3. **信号重入安全 (Signal Re-entrancy)**：
   - 在 GDScript 连接的 `doc.changed` / `doc.preview_changed` 信号回调中，允许同步安全调用只读查询方法（如 `get_document_summary()`、`get_mesh_snapshot()`、`get_frame()` 等）。
   - 内部 Rust 绑定不得在向 Godot 派发信号时死锁或触发 `BorrowMutError` 崩溃。
4. **内存闭环**：
   - 循环执行 50-100 次完整的工程加载、编辑、预览与释放，引擎对象计数与进程 RSS 必须保持收敛，无孤儿节点与无界内存泄漏。

---

## 4. 官方 Live2D Cubism Core 对照与兼容指标

1. **MOC3 5.0 二进制规范**：
   - Rust 导出的 `model.moc3` 文件必须严格符合 Cubism Core 5.0/6.0 内存布局（64 字节对齐、标准节计数与偏移量）。
   - 必须通过官方 SDK 的一致性校验（`csmHasMocConsistency` 返回 1）。
2. **官方 Core 几何与绘制一致性**：
   - 顶点变形位置：在统一 PPU（Pixels Per Unit）基准下，Rust 求值结果与官方 Core 采样误差必须 $\le 0.05$ 像素（或相对浮点误差 $\le 10^{-4}$）。
   - 渲染属性：`runtime_id`、`texture_slot`、`draw_order`、`render_order`、`double_sided`、`inverted_mask`、`blend_mode`、`mask_indices` 必须 100% 逐项吻合。
   - 颜色与透明度：综合透明度（`opacity`）、正片叠底颜色（`multiply_color`）、滤色颜色（`screen_color`）误差必须 $\le 10^{-4}$。

---

## 5. GPU 视觉回归阈值 (Visual Acceptance)

在真实 GPU 环境下，`KasaneDocumentPreview` 与官方 Core `GDCubismUserModel` 画面比对标准：
1. **全画幅指标**：平均颜色差值 $\text{mean} \le 0.005$，坏点比例 $\text{bad\_pixel\_fraction} \le 0.01$。
2. **局部区域指标**：在反向遮罩、普通遮罩、三种混合模式（Normal / Additive / Multiply）的特定 $5 \times 5$ 分析区域内，最大单通道像素误差 $\le 2/255$。

---

## 6. 与旧 C++ 实现的已知差异裁定记录

| 差异项 | 旧 C++ 实现表现 | Rust 实现表现 | 裁定标准与原因 |
|---|---|---|---|
| 缺失 `decimal_places` 的旧工程 | 隐式使用硬编码 6 | 自动反序列化缺省为 6 | **保留并规范化**：保证历史合法文件向后兼容 |
| 变形器循环挂载 | 已有 RELATION_CYCLE 检测 | 拓扑检查拒绝循环 | 保持既有安全行为 |
| 参数范围收缩 | 候选文档重新校验 MeshBinding/SceneBinding | 同样重新校验 | 修复初版 Rust 移植回归，非 C++ 缺陷 |
| 句柄过期调用 | ObjectDB 与 generation 校验，返回 STALE_HANDLE | 同样检查句柄世代 | 保持既有安全行为 |

初版 Rust 移植中的源工程覆盖等问题属于移植审查发现，不应据此推断旧 C++ 存在相同缺陷。
