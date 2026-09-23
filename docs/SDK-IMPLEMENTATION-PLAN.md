# Kasane 创作 SDK：调研结论与实施计划

状态：实施中；第 13 节的 Rust 最小纵切已实现，其余阶段待交付。调研日期：2026-09-23。代码基线：`8b283f8eeca785b506e6dbae0552cc2358f49053`。

本文把已验证的仓库事实、建议采用的设计和未来验收分开记录。当前实际 API 与未完成项见 [SDK-API.md](SDK-API.md) 和 [SDK-COVERAGE.md](SDK-COVERAGE.md)；本计划其余新 crate、目录和命令仍是待交付内容。用户已明确的约束是：不复用或修改 `apps/editor`；新建 crate；暂不选择或实现交互层；先提供覆盖原 Document Bridge 能力的 SDK，使 agent 能完成建模、编排和测试。

## 1. 交付目标与范围

交付一个可在没有 Godot、窗口和 UI 上下文的环境运行的创作 SDK。Rust API 是编辑语义的唯一实现；Python 是首个面向 agent 的创作入口；wgpu 观察宿主输出可信的图像和数据证据。未来 editor 调用同一 SDK。

第一版完整交付需做到：读取或创建工程 → 查询模型 → 批量修改几何及绑定 → 检查参数采样 → 渲染观察 → 持续修正 → 保存重开 → 按已有能力导出。用户能把一次创作保存为脚本，并把重要约束写成测试。

本轮包含：

- Document Bridge 全部领域能力、工程 IO、资源诊断、完整文档编辑的撤销与重做。
- 强类型 Rust API、Python 对象接口、类型提示、异常、可执行示例。
- 无窗口 CPU 开发 harness、独立 wgpu 图像观察、结构化验收报告。
- 少量确定性的创作辅助：PNG 元数据导入、矩形/规则网格、完整 keyform 表构建。

本轮不包含：UI 框架、选择工具和拖拽交互、改造旧 editor、通用 RPC/MCP 服务、嵌入式 Python 桌面应用、自动绑定/自动网格生成算法、时间线/动画剪辑等 core 尚无的模型能力。这里的“编排”指现有 part、变形关系、绘制顺序、mask、offscreen、参数及 keyform 的组织。wgpu 已验收的渲染规则保持不变。

## 2. 仓库调研：可复用部分与实际缺口

| 事实与代码证据 | 实施含义 |
| --- | --- |
| [Document Bridge](../modules/kasane-godot/src/document_bridge.rs) 是 Godot 门面，包含 generation、object epoch、预览缓存、变更发布和分模块编辑入口 | 用作能力清单；不迁移 Godot 类型和主线程约束 |
| [Document](../modules/kasane-core/src/document.rs) 已独立于引擎，领域操作在 `document/` 下 | 复用验证和求值；不在 Python 再实现一套规则 |
| [DocumentSession](../modules/kasane-project/src/store.rs) 拥有文档、历史、manifest 路径和保存基线 | SDK 可以持有它；需明确 SDK 与旧 history 的单一所有权 |
| [History](../modules/kasane-core/src/history.rs) 的 delta 仅支持 `MeshName` 和 `Vertex`；无 receipt 的编辑触发 `HISTORY_UNSUPPORTED_EDIT` 并清空历史 | 不能声称现有 begin/end action 已能撤销创建对象、绑定或资源替换 |
| [transaction](../modules/kasane-core/src/document/transactions.rs) 仅暂存顶点位置和 mesh 名称；其它修改在 transaction 激活时被阻止 | SDK 的完整原子编辑必须新实现，不能简单包装旧 transaction |
| `Document::restore_from` 保留当前 saved baseline，并推进 revision；`Document::clone` 同时涉及派生缓存和 saved content | 新增不含缓存/历史/保存基线的内容 checkpoint，避免直接深拷贝完整 Session |
| [PreviewState](../modules/kasane-core/src/preview.rs) 用 generation、evaluation revision、preview revision 缓存不可变帧 | 延续区分；普通 metadata 修改可能复用较早求值帧 |
| [valid_uuid](../modules/kasane-core/src/document/validation.rs) 要求非零、规范小写 UUID | `eye.left` 是显示名/脚本符号的示例，不能直接作为底层对象 ID |
| [binding 验证](../modules/kasane-core/src/document/bindings.rs) 要求完整笛卡尔积 keyform；mesh binding 支持 1–16 个轴 | 不能先发布空 binding，再逐个补 keyform；在 builder 内组装完整值后提交 |
| [to_parent_positions](../modules/kasane-core/src/evaluation/types.rs) 对无父对象进行 canvas → runtime 转换，有父对象则保留父空间坐标 | 不能把所有源顶点统称为画布像素；reparent 必须说明坐标语义 |
| [parameters 求值](../modules/kasane-core/src/evaluation/parameters.rs) 普通参数钳制，repeat 参数循环映射 | 返回 requested 和 actual；不能给所有参数统一承诺 clamp |
| [ProjectIO](../modules/kasane-godot/src/project_io.rs) 在 bridge 之外承载打开、保存、资源替换、MOC3 IO | 仅迁移 `document_bridge.rs` 不能组成完整 SDK 闭环 |
| `DocumentStore::save_inner` 复制资源、改写 source/hash，成功后替换当前文档；保存带外部修改检测和发布后 warning | 全内容历史必须处理保存后的资源路径；不得破坏既有 IO 发布语义 |
| [kasane-preview](../modules/kasane-preview/src/lib.rs) 已有资源检查协议；[wgpu 验证宿主](../apps/wgpu-validate/src/main.rs) 已有 device、纹理上传和读回路径 | 可以写独立观察宿主，不需要窗口或 editor |
| [wgpu 验证库](../apps/wgpu-validate/src/lib.rs) 的 `LoadedModel` 以 model3 case 为输入，不能直接观察未保存的 SDK 文档 | 新观察层直接消费 SDK 快照，不通过导出后再导入绕行 |
| [kasane-moc3](../modules/kasane-moc3/Cargo.toml) 默认启用 Purism 验证；project 依赖它 | “无 Godot”不等于构建没有 C 编译器/子模块依赖，打包必须记录实际能力 |

已有 core 的公开操作包括 canvas 和 draw-order groups，范围略大于旧 bridge。第一版一并提供读取和显式替换，避免导入再保存时丢失这些语义。

### 2.1 本次实际运行的基线

环境：本地 `rustc 1.98.1`、`Python 3.14.3`。workspace 声明 Rust 最低版本为 1.94；本次没有在该最低版本验证。

```sh
cargo test -p kasane-core --locked --test history_tests --test preview_state_tests
cargo test -p kasane-project --locked --test project_tests --test sequence_tests
```

结果：6 个 history、5 个 preview、38 个 project、1 个 sequence，共 50 个测试通过，无跳过。覆盖了历史边界、预览失败保留状态、保存冲突、保存前失败、发布后 warning、save-as 和既有 delta history 跨保存等行为。这不是新 SDK 验收，也没有重新验收 GPU 或官方 Core。

## 3. 外部方案调研与选型

| 方案 | 对本项目的判断 |
| --- | --- |
| Rust SDK + CPython 扩展 | 采用。Rust 保持领域约束，Python 组织创作算法和测试；脚本运行时无需逐次编译 Rust |
| 只提供 Rust API | 作为第一阶段交付点；不足以完成最终 agent 创作入口 |
| CLI/JSON 命令作为唯一 API | 不采用为主入口。大型数组和组合算法需要额外序列化，脚本表达能力仍要另建；CLI 仅负责运行和证据收集 |
| 在 Rust 应用中嵌入 Python | 暂缓。当前没有需要嵌入的 UI 应用；先让 Python import 原生库即可 |
| 另选嵌入式脚本语言 | 暂缓。此时增加语言适配与生态学习成本，没有已确认的宿主约束需要它 |

Blender 的直接数据访问和可组合脚本值得借鉴；其 operator 的上下文依赖和对象底层数据生命周期则说明，SDK 应显式传入目标并校验句柄，不依赖活动对象或当前面板。参见 [Using Operators](https://docs.blender.org/api/main/info_gotchas_operators.html) 和 [Internal Data & Their Python Objects](https://docs.blender.org/api/5.0/info_gotchas_internal_data_and_python_objects.html)。

Python 绑定建议用 PyO3，wheel 用 maturin 构建。混合 Python/Rust 布局可将原生模块命名为 `kasane._native`，并交付 `.pyi` 和 `py.typed`；这是 maturin 直接支持的布局。[maturin Project Layout](https://www.maturin.rs/project_layout.html)

线程安全由原生层明确实现。PyO3 文档指出 Python 对象可能跨线程共享，单靠 GIL 或 `unsendable` 不是通用方案；采用短时锁与显式 edit token，禁止跨 Python 调用持有 Rust 可变借用。[PyO3 thread safety](https://pyo3.rs/v0.29.0/class/thread-safety.html)

首个验收平台设为本地 macOS arm64 + 常规 CPython 3.14。先构建对应解释器 wheel；多 Python 版本、abi3 和 free-threaded wheel 后续单独验收，不从一次安装成功推导兼容性。PyO3 的构建文档区分扩展模块、嵌入和稳定 ABI，并提示链接配置会影响 Rust tests；实施时固定一组已发布且经冒烟验证的 PyO3/maturin 版本，不直接依赖 `main` 或盲抄旧 `extension-module` 配置。[PyO3 building and distribution](https://pyo3.rs/main/building-and-distribution.html)

## 4. 目录与依赖边界

按阶段新增以下内容；不是第一天同时建立所有空壳。

```text
modules/kasane-sdk/                 # S1：会话、对象 API、编辑、查询、历史
  src/{session,edit,history,handles,query,error,events,validation}.rs
  src/authoring/                   # 按 mesh/parameter/binding 等领域分文件
  tests/                           # 仅使用公开 API 的契约测试
modules/kasane-sdk-observe/         # S4：无窗口 wgpu 观察、PNG 和报告
modules/kasane-python/             # S3：薄 PyO3 绑定
  Cargo.toml
  pyproject.toml
  python/kasane/{__init__.py,_native.pyi,py.typed,__main__.py}
  tests/
examples/sdk/                      # 可分发素材、小型完整创作脚本和测试
tools/validate_sdk.py              # S5：统一验收入口
docs/SDK-API.md                    # 随 API 更新的使用契约
docs/SDK-ACCEPTANCE.md             # 实际命令、产物和验证结果
```

依赖方向：

```text
未来 UI ───────────────> kasane-sdk ──> kasane-project ──> kasane-core
Python ─> kasane-python ─────┘
                 └──> kasane-sdk-observe ──> kasane-sdk（只读观察输入）
                                  └───────> kasane-preview / render / render-wgpu
```

- `kasane-sdk` 默认不依赖 Godot、wgpu、窗口库或 Python。
- `kasane-sdk-observe` 拥有 GPU device/queue、纹理缓存、读回和观察产物；不拥有编辑权。
- Python crate 用可选 `observe` feature 接入观察层。首个开发 wheel 可包含该 feature，但 `import kasane` 不初始化 GPU。
- 不让 SDK 依赖 `apps/wgpu-validate`。以它作为宿主实现参考；需要共享的通用资源代码抽到合适的 module，避免反向依赖 app。
- `kasane-moc3` 的已有验证 feature 不在这一轮静默关闭。CPU 测试的最小依赖方案若需 feature 透传，应作为独立小改动，并检查 Cargo feature 合并后的实际构建。

## 5. 会话与公开接口契约

### 5.1 所有权

`AuthoringSession` 持有 `DocumentSession`、`PreviewState`、SDK history、session ID、generation、对象 incarnation 和事件队列。公开只读快照与受控操作，不导出可绕过历史的 `&mut Document` 或 `&mut DocumentSession`。

SDK history 是此会话唯一的撤销管理者。SDK 路径不调用旧 `DocumentSession::edit/record_edit/begin_action` 来维护第二份历史；内部采用受限候选文档发布入口。旧 Godot 路径及旧 history 行为不变。

Rust 默认用 `&mut self` 串行编辑；Python 使用 `Arc<Mutex<AuthoringSession>>` 的薄封装。每个调用短暂加锁；不持锁运行用户 Python、发事件回调或等待 GPU。实现阶段用编译检查确认 `Send` 约束，不使用手写 `unsafe Send/Sync` 绕过不满足的约束。

### 5.2 标识与查询

- `ObjectId`：持久化 UUID。SDK 默认生成，也允许脚本提供合法 UUID。
- `name`：允许重复的显示名。`find(name=...)` 返回集合；`require_unique(name=...)` 在零个或多个结果时给出不同错误。
- `runtime_id`：导入/导出的运行标识，遵循现有 core/export 约束，不代替对象 ID。
- v0 不新增持久化 alias/tag 系统。可重复构建脚本可使用固定 namespace 生成确定性 UUID；持续修改脚本保存 ID 清单，并校验目标文档 UUID。
- 禁止默认 upsert：create 遇重复 ID 报错；replace 必须明确调用。

对象句柄包含 `(session_id, generation, object_id, incarnation, kind)`。读取时解析 ID，不保存指向 Document 内部集合的裸引用。

成功打开/新建/导入另一文档增加 generation；失败不变。删除、同 ID 重建和对象经 undo/redo 消失或恢复时更新 incarnation，旧句柄永不复活。普通字段修改保留句柄；拓扑快照另携带 revision，写回必须做过期检查。仅存在于未提交 edit 的对象使用 edit-scoped handle，提交后按 ID 重新取得正式句柄。

查询至少包括：summary、按类型列举、分页/字段投影、对象快照、组织父子关系、变形父子关系、bindings、直接 referrers。组织树和变形图分开表达。间接影响查询是额外能力，不能把直接 referrers 宣称为完整影响集合。

### 5.3 数据与坐标

Rust 入口采用明确的描述结构和 `Result<T, SdkError>`。可以复用 core 值类型，但 SDK 对外版本变更需记录；Python 不接受任意缺字段字典充当所有对象类型。

几何快照提供 `vertex_ids`、`positions`、`uvs`、`triangles` 和 `space`。positions 使用有限 float32；RotationPose 的源 origin 保留现有 float64 精度。源 triangles 是 `VertexId` 三元组，求值帧 indices 是 `u32` 数组下标。数组顺序有契约，不能自动当作稳定身份。

源空间区分 `CanvasPixels` 和 `ParentLocal(parent_id)`；求值输出为 `Runtime`。根空间转换沿用 `(x-origin.x)/ppu`、`(origin.y-y)/ppu`。父空间由 Rotation/Warp 的现有求值规则解释，不假设所有父空间都有统一像素单位。旋转角沿用 core 的度数。UV 保持 core 约定，renderer 的翻转不写回源数据；通过非对称角标纹理固定端到端契约。

修改父关系的低层接口明确为保留源数值，会改变其空间解释；`keep_world` 需要逆变形求解，不在 v0 暗中实现。高级辅助只能在有明确转换的情形提供，无法转换时返回错误。

Python 读数组返回独立副本；修改副本不会修改文档，批量写回须显式调用。首版支持普通序列；连续数组/buffer 快路径可随后加入，必须验证 dtype、shape、stride、长度和转换溢出。不提供指向可编辑文档内存的 writable 零拷贝视图。

### 5.4 版本、错误与事件

沿用文档 revision，不再创建另一套同名 revision。`Version = { session_id, generation, revision }`；批量操作可附 `expected_version`。一次候选发布推进一次文档 revision；保存可能因资源改写推进多次，以返回值为准。版本保证递增，不要求连续。

错误至少包含 `code`、`message`、`operation`、`object_ids`、可选 `field_path`、`expected/actual_version` 和修复所需的 referrers。保留 core 错误码；SDK 新增码采用明确命名，例如 `STALE_HANDLE`、`EDIT_ACTIVE`、`EDIT_ABORTED`、`AMBIGUOUS_NAME`。无法从现有 Status 获得字段路径时留空，不通过解析人类 message 伪造结构化信息。

Rust 返回 edit receipt；Python 失败抛 `KasaneError`，带相同字段，不能只打印日志。成功 receipt 含前后版本、直接修改对象、change kind、history 和 warnings。事件分为 document、preview、project/resource、history；首版提供 `drain_events()`，不在锁内同步调用用户回调。结构和混合修改允许保守标记 Structure；直接修改的 mesh 列表不是 renderer 的完整 dirty 集合。

## 6. 原子编辑、历史和保存

### 6.1 候选文档方案

首版采用内容 checkpoint + 隔离候选文档。优先实现可验证的语义，再根据 benchmark 将高频编辑替换成等价 delta 路径。

建议新增 core 的不透明 `DocumentCheckpoint`：包含全部持久化内容与对象顺序，不含 revision、saved baseline、lookup/prepared 缓存、receipt 或 transaction 状态。由 core 提供捕获、内容比较、估算内存和恢复方法；不把私有 `DocumentContent` 的字段暴露成公共可变结构。

候选发布步骤：

1. 校验当前 generation/revision，检查无活动 edit，检查编辑内存预算。
2. 捕获当前内容并建立 candidate，分配唯一 edit token。已提交文档保持可读。
3. 所有 `EditSession` 方法只修改 candidate；直接 `session` 写入、嵌套 edit、save/open/import/undo/redo 返回 `EDIT_ACTIVE`。
4. 单次方法仍使用 core 的验证。任一编辑方法失败将 edit 标为 aborted，即使 Python 捕获了异常，后续 commit 也失败；需要从头开启新 edit。
5. builder 在普通内存中组装完整 mesh/binding/关系描述，再按依赖顺序调用 core。v0 不支持悬空引用的暂时可见模型，也不允许先发布不完整 keyform。
6. 提交时验证最终结构、版本和历史容量。校验不要求纹理文件此刻可读；结构合法性与资源完整性分开报告。
7. 内容与开始时相同且没有实际身份更替则 no-op：不推进 revision、不产生 undo entry、不清空 redo。同 ID 删除重建即使内容相同也不是身份 no-op。否则通过受限发布入口一次替换已提交内容，保留当前 saved baseline，产生一条历史和一条合并变更事件。
8. 丢弃、异常退出和正常取消仅销毁 candidate，不改变文档、预览、历史、revision 或已发布事件。drop guard 保证 Rust 提前返回可清理；正常 Python 异常走 `__exit__` 清理。

对 Rust 提供闭包便利入口 `session.edit(label, |edit| -> Result<...>)`，底层仍为 begin/commit/abort。Python 使用 `with session.edit(label) as edit:`；锁不跨越 with 块，edit token 校验阻止其它线程混入该批次。批次线程切换返回结构化错误；不宣称线程并行编辑同一文档。

单个公开修改操作默认等价于一项原子 edit；循环大量修改应显式组织为一个 edit，从而只建立一次 candidate。这个性能约束写进示例和 API 文档。

候选发布不能直接用现有 `restore_from` 包办所有情况：它会一律推进 evaluation revision。新增发布入口应根据内容差异判定变化类别，纯名称等 metadata 提交保留 evaluation revision；结构/几何/资源变动使相关缓存失效。undo/redo 的首版允许保守失效。类别来自受控编辑及内容校验，不接受外部任意指定以跳过失效。

文件保存、导出和观察不属于文档事务。素材导入可以读取/解码外部文件并在 candidate 中登记带 hash 的描述，但不能在 edit 中发布工程文件。用户脚本自己的文件写入不回滚。进程被终止没有“内存 edit 已优雅取消”的保证；未显式保存的内存修改不会自动写入工程。

### 6.2 完整历史

SDK history 首版使用可交换内容 checkpoint：当前文档持有一侧，history entry 持有另一侧；undo/redo 原子恢复并交换 checkpoint。预算覆盖 done、redo、pending 与 candidate，不能只统计序列化文本大小。建议初始配置为最多 50 步、history 256 MiB，最终默认值在 S1 测量后固定；单次 candidate 峰值另计并报告。

历史预算不足时，先计算淘汰最旧条目的方案；只有提交成功才执行淘汰。若新条目自身超过上限，默认在发布前失败，保留当前文档和历史。可以单独提供明确的无历史编辑模式，但不得把正常编辑悄悄降级为不可撤销。估算不等于进程 RSS 硬限制，报告两者区别。

撤销覆盖所有公开编辑对象，而不是仅覆盖顶点。内容恢复后 revision 增加，预览缓存失效，临时参数只保留仍存在的参数 ID；同 ID 参数的范围变化需重新求值。句柄 incarnation 来自提交/重放的生命周期记录，不能只比较最终 ID 集合：同一批次删除后同 ID 重建，也必须使旧句柄失效。

原来的 stage/commit vertex 接口保留为 SDK 批量位置方法的兼容映射，不把旧 core transaction 和新 edit 同时暴露为可嵌套事务。begin/end/cancel action 的能力由有名称的 edit 提供；旧名称可作为迁移对照，不要求函数签名兼容 Godot。

### 6.3 保存后历史必须仍然正确

这是 S2 的阻断性验收项。不能直接用旧整文档快照覆盖保存后的文档，也不能把成功 save 当作默认 history barrier：现有工程测试已保证部分历史跨保存可用，新 SDK 不应无提示退化。

实施方案是在成功保存后对所有历史 checkpoint 做资源路径重定位，保留它们的创作内容：

1. 保存前记录当前 source root、所有资源描述及历史 checkpoint 所属 root。SDK 导入/替换素材时填入实际 SHA-256。
2. 调用现有 `DocumentSession::save`。失败保持文档、manifest 基线和 SDK history；发布成功但目录 sync warning 的情况按成功处理并保留 warning。
3. 由保存前后文档生成 `ResourceRelocationMap`。匹配键包含 asset ID、解析后的旧 source、尺寸与已验证内容身份，不能只按 ID 匹配，也不能把相同 ID 的旧纹理误改成新纹理。
4. 历史资源若与已发布资源匹配，仅改写其存储位置/hash 表达为新工程中的对应值，保留历史名称等其它字段。历史中独有的资源保留内容身份，其相对路径转为原 root 下的绝对路径，避免在 save-as 后被解释到新 root。
5. 重定位是由已捕获描述驱动的纯内存操作，不再次读取磁盘；更新所有 done/redo checkpoint 的 root 上下文。恢复 checkpoint 时始终保留当前会话的 manifest、保存冲突基线和 saved content。

core 为 checkpoint 提供受限资源重定位能力；文件路径解析和映射由 project/SDK 完成。不得让 checkpoint 包含可恢复的 `manifest_sha256`。对于旧工程中 hash 为空的资源，其历史语义只是路径引用：只有旧 source/尺寸等描述完全匹配时才采用保存此次验证得到的映射，报告先前内容身份未验证，不承诺还原过去的文件字节。旧文件被外部删除或篡改时，undo 可以恢复逻辑引用，但资源诊断必须明确失败；历史不承诺恢复外部文件字节。

必须覆盖：位置修改→save→undo→redo；资源 A→B→save-as→undo(A)→redo(B)；历史对象已删除；多个历史版本共用 asset ID 但内容不同；未带 hash 的旧资源；相同内容但显式 relocate；失败保存；发布后 warning。redo 回已保存内容时 `modified=false`，不能因绝对/相对路径表达差异误报 dirty。

## 7. 旧能力到新 SDK 的迁移清单

表中名称为接口族，最终 Rust/Python 签名在 S0 固化到 `SDK-API.md`。create 与 replace 分离，不用布尔开关隐藏覆盖行为。每行需有正常和失败契约测试。

| 旧入口/能力 | 新接口族与要点 | 阶段 |
| --- | --- | --- |
| initialize / new_project / document state / summary | Session 新建、原子替换、查询；失败不丢当前工程 | S1 |
| add_image_asset / asset snapshot | assets.create/get；增加读 PNG 尺寸/hash 的导入辅助 | S1/S2 |
| create/replace mesh、properties、rename | meshes.create/replace/update_properties/rename；保留 runtime ID 等未修改字段 | S1 |
| vertex positions、stage、commit at revision | meshes.update_positions、edit 批量提交、expected_version | S1 |
| topology snapshot / replace topology | 显式 vertex mapping；保留或拒绝无法迁移的绑定/glue，复用 core 规则 | S1 |
| replace_mesh_with_keyforms | mesh + 完整 binding 同一操作发布 | S1 |
| create/replace parameter、parameter samples | parameters.create/replace/get；普通与 repeat 行为一致 | S1 |
| rotation / warp / write_transform | typed Transform 描述、pose/control points 更新，完整读写 | S1 |
| deform parent / organization parent | 分开的关系 API；坐标语义明确，不自动 keep-world | S1 |
| write_part / part binding with offscreen | parts 与现有复合替换操作，避免降级成多次非原子写入 | S1 |
| write_binding / mesh keyform | 完整 MeshBinding builder、按参数关键值读取/写入 | S1 |
| write_scene_binding | Rotation/Warp/Part 三种 SceneTrack，完整形态表和指定 keyform 更新 | S1 |
| blend key table / constraint / binding | 各自独立 create/replace/get，复用 DeltaKeyforms 类型 | S1 |
| write_glue / write_offscreen | 完整描述、引用、属性与各自内嵌的 binding/keyforms；不误当作 SceneTrack | S1 |
| references_to / erase | 显式拒绝仍被引用的删除；不默认级联删除 | S1 |
| begin/end/cancel action、undo/redo | SDK edit、完整 checkpoint history、可检查的容量和标签 | S1/S2 |
| set/reset preview values、frame、evaluate mesh | preview 与一次性 evaluation；不写回模型 | S1 |
| project open/save/import/export/diagnose/relocate/replace | project/session IO；保留冲突、publication、资源诊断和 import report | S2 |
| core canvas / draw-order groups | 读取与显式修改，保存/导出往返不丢失 | S1/S2 |
| 全图/对象观察（原应用能力） | observe，附带版本、采样、资源和视图证据 | S4 |

S0 生成逐方法 coverage 文件，将 bridge 的每个公开领域方法映射到新 API 和测试名；内部 `apply/session_mut/base` 等机制明确列为不迁移。不能只用本表的概括覆盖代替逐项检查。

## 8. Python 创作接口与运行 harness

### 8.1 编程体验

创建操作返回带 ID 的结果或句柄；读取是快照；写入是明确的方法调用。Python 便利函数负责参数组织和 builder，领域验证、版本检查、原子提交仍调用 Rust。

以下为待实现的目标示例。它使用完整 keyform 描述，不先创建不合法的空 binding；名称查询显式要求唯一。

```python
from pathlib import Path
import kasane as ks

session = ks.open_project(Path("character").resolve())
eye = session.meshes.require_unique(name="eye.left")
source = eye.geometry()  # positions 是副本，带 space 和 version
closed = [(x, 0.8 * y + 0.2 * source.positions[0][1])
          for x, y in source.positions]  # 仅示例形变，不是通用闭眼算法

with session.edit("制作左眼参数形态", expected_version=source.version) as edit:
    parameter = edit.parameters.create(
        name="左眼开合", minimum=0.0, maximum=1.0, default=1.0
    )
    edit.mesh_bindings.create(
        mesh_id=eye.id,
        axes=[ks.Axis(parameter.id, keys=[0.0, 1.0])],
        forms=[
            ks.MeshKeyform(keys=[0.0], positions=closed),
            ks.MeshKeyform(keys=[1.0], positions=source.positions),
        ],
        space=source.space,
    )
    parameter_id = parameter.id

session.validate_structure().raise_for_errors()
samples = [{parameter_id: value} for value in (0.0, 0.25, 0.5, 0.75, 1.0)]
with ks.Observer() as observer:
    report = observer.observe(session, samples=samples, focus=[eye.id],
                              output=Path("artifacts/blink").resolve())
report.raise_for_errors()
session.save()
```

示例假定目标 mesh 尚无冲突 binding；修改已有绑定的脚本必须先查询并显式 replace。第一段脚本创建结果的 ID 可写入用户指定的 recipe 输出文件，第二段脚本按 ID 继续编辑。重复运行 create 不自动覆盖。

### 8.2 开发与执行入口

普通使用方式为 `python create_model.py` 和标准 Python 测试。另交付 `python -m kasane run <script> --report <path>`：编译脚本、收集异常栈/行号、stdout/stderr、会话版本和 observation 产物索引。runner 不自动给整份脚本包事务；显式 with edit 内回滚，已经提交的批次保留。

开发时支持在普通 Python REPL 中长期持有 Session 和 Observer，反复查询、编辑和观察；仅修改创作脚本无需重编译 Rust。示例按“创建 fixture → 执行 edit → 数值断言 → 可选观察”拆成可组合函数，agent 可只运行相关测试。首次 wheel 编译成本与后续脚本迭代耗时分别报告。

runner 是一次性进程；不建立后台 daemon，也不提供逐顶点网络调用。超时由外层进程管理器终止，并报告 `terminated`，不能伪称所有外部副作用已撤销。只有显式 save/export 发布文件；退出不自动保存。

Python 分发要求：

- maturin mixed layout；公开 `kasane`，原生扩展 `kasane._native`。
- `.pyi`、`py.typed`、docstring、异常字段、数组复制规则和可运行 recipes 随 wheel 交付。
- Rust 依赖进入 Cargo.lock；Python 构建/测试工具精确锁定。版本选择需验证 workspace MSRV 和本地 CPython。
- 在仓库外新 venv 安装 wheel，清空仓库 `PYTHONPATH`，运行 CPU 创建/保存/重开和 GPU 观察样例。
- 不将官方 Core 二进制或不可分发模型打进 wheel。`capabilities()` 返回实际可用的导入/导出验证与观察 feature；GPU 是否可用须实际探测。
- 释放 GIL 的长操作不得持有 Python 引用或调用 Python 回调；获取会话锁与 GIL 的顺序须按固定 PyO3 版本验证，包含双线程冲突测试。

## 9. 求值、观察与诊断

### 9.1 两种求值入口

`session.preview.set_values()` 更新会话临时预览；`session.evaluate(values)` 对明确参数快照求值，不改变当前 preview。后者用于测试和 sweep，避免最后一个测试样本污染用户的预览状态。返回 requested/actual 参数、求值结果和版本标识。

`validate_structure()` 验证内容和引用；`diagnose_resources()` 检查资源；`evaluate()` 检查指定参数的求值。三者分别报告。结构完整不保证所有姿态外观合理；存在缺失文件不等于文档结构不可编辑。新增 core 全结构校验入口应复用已有 validator，并覆盖从 checkpoint 恢复的路径；不能把 `encode_project` 成功当作完整结构验证。

三角形翻转、过小面积、超出预期 bounds 等是创作诊断，默认 warning 或脚本可配置断言。它们不是现有所有合法模型都必须拒绝的错误。自动诊断不能证明闭眼自然、遮挡合理或作品完成。

### 9.2 观察输入与执行

`Observer` 接收不可变 `ObservationInput`：内容/求值快照、session/generation/document/evaluation 标识、明确参数、资源描述和 root、view、输出尺寸、纹理 profile。短时捕获后释放编辑锁；异步完成的结果始终标注捕获版本，不被重新标成最新版本。

一次观察按以下顺序执行：

1. 捕获文档与资源描述。若有未结束 edit，默认观察已提交文档；v0 不提供 candidate 渲染。
2. 明确采样参数，使用独立 evaluator，记录普通参数钳制、repeat 映射后的实际值。
3. 校验并读取纹理，hash 计算与上传使用同一份字节，避免检查后重新读取造成不一致。缺失/hash/尺寸错误明确失败。
4. 复用 `kasane-preview` 的资源校验规则。必要时在该 crate 加入只读资源来源抽象，使 snapshot 和旧 DocumentSession adapter 共用实现；不复制第二套资源判定策略。
5. 调用现有 `WgpuRenderer::sync_model/update_view/encode`，提交 GPU 工作，等待对应读回完成，去除 row padding，输出 PNG 和报告。
6. 每个观察 run 使用独立目录，图像成功后才将 manifest 标为完成；失败报告不引用上一 run 的图片作为本次结果。

Renderer、纹理和 device 在 Observer 生命周期内复用。资源版本取实际内容身份，不沿用 demo 中“永远 revision=1”的假设。设备不可用返回可诊断错误；对验收记录为 `not_run` 并使必需 GPU 门禁不通过。

### 9.3 产物契约

```text
artifacts/<run-id>/
  report.json
  samples.json
  frames/000.png ...
  crops/<object-id>/000.png ...
  contact-sheet.png
  diagnostics.json
  overlays/                       # 可选，独立于干净图
```

报告至少记录 SDK/报告 schema 版本、源码 revision/工作区指纹、平台/adapter/backend、输入 hash、session 与文档 UUID、generation、文档 revision、求值 source revision、实际采样、canvas/坐标空间、view/尺寸、资源 hash、纹理 profile、输出色彩/alpha 约定、图片 hash、逐项 expected/actual 和失败 ID。

`DrawableFrame.source_revision` 可能早于当前 metadata revision；报告同时保留二者及 evaluation revision，不通过修改帧版本伪造重新求值。缓存复用也必须能证明图像适用于被标注的内容。

对象 crop 默认从完整场景图裁剪，保留原有遮挡、mask 和 offscreen 合成。focus bounds 采用本次求值后的几何与 view；不能“只渲染目标对象”却把破坏了依赖的画面当作原场景局部。不可见/无几何对象返回明确诊断。wireframe/ID overlay 单独输出，不进入干净图像对照。

固定输出透明背景、分辨率、view 和纹理 profile 才可进行图像差异断言。沿用 wgpu 已验收的颜色流程，并在 PNG 规范中写明读回 alpha 表达及任何转换；不能在新宿主悄悄增加 gamma 或 premultiply 变换。

## 10. 分阶段实施与完成门槛

每阶段提交代码、公开 API 示例和独立验收结果。S1 完成可称 Rust 编辑 SDK；只有 S1–S5 的必需项全部通过，才能称 agent 创作 SDK 完整交付。

### S0：固定契约与实施基线

产物：`SDK-API.md` 初稿、逐方法 coverage 清单、可分发最小 fixture、Python 构建版本锁定方案。

工作：

- 固定对象/版本/坐标/error/receipt/handle/edit 的签名，按第 7 节清点所有 bridge 与 ProjectIO 方法。
- 检查 root/parent 空间、UV 和 keyform builder，建立非对称纹理与两种变形嵌套 fixture。
- 用独立最小绑定验证所选 PyO3/maturin、CPython 3.14、macOS arm64 和 Rust 1.94 的兼容性；失败则选兼容版本或记录明确的 MSRV 调整，不修改 renderer 技术选型。
- 测量小/大模型内容 checkpoint 大小及构造成本，固定 S1 的内存预算和 S5 的性能比较口径。

通过标准：实施者不必猜测 ID、空间、数组所有权、事务失败和方法覆盖；绑定可在仓库外 import；任何新增依赖均有实际锁定版本。

### S1：纯 Rust 编辑 SDK

新增 `modules/kasane-sdk`；在 workspace 注册。新增 core opaque checkpoint、结构校验和必要的候选发布支持；在 project 增加保留 manifest/saved baseline 的受限发布入口。新增接口需明确是否影响 legacy history，不能让两套记录器混用。

完成第 7 节标记 S1 的所有领域接口、受控写入、edit token、rollback、history、句柄、查询、preview/evaluate、error 和事件。checkpoint 估算包含 Vec/String capacity 等动态占用，不通过 clone 保存每个操作的 GPU/求值缓存。

建议拆成五个可独立审查的实施批次：S1a checkpoint/会话/版本/历史内核；S1b assets/mesh/vertices/topology；S1c parameters/transforms/parts/bindings；S1d blendshapes/glue/offscreen/canvas/order；S1e 完整 coverage、诊断、失败矩阵与性能测量。S1b–S1d 均通过同一发布入口，禁止各自建立历史或事件捷径。

通过标准：

- 纯 Rust integration tests 仅通过 SDK 公开 API 创建完整模型、修改、求值和撤销。
- 创建 mesh/parameter/binding 后在同一 edit 第 N 步制造错误，状态/版本/历史/事件全部保持原样。
- undo/redo 覆盖所有领域对象；失败和 no-op 不清空 redo；超预算在发布前失败。
- 跨 session、换文档、删后同 ID 重建、undo 后重现、过期 topology snapshot 均被正确处理。
- 核心依赖图不含 Godot/Python/wgpu；50 项本次基线保持通过。

### S2：工程与资源闭环

接入 new/open/save/save-as/model3/bare moc3/import report、资源诊断/relocate/replace、export。素材辅助读取尺寸和 hash，再提交合法描述；相对用户路径在 SDK 入口按显式 base directory 转为 native absolute path，不依赖临时改变 cwd。

实现第 6.3 节 history 资源重定位；保存路径更新走 project 原有 publication 机制。可增加 prepare-resource 方法，将“读文件得到描述”与“写入 Document”分开，供 candidate 编辑复用，避免临时修改真实 session 再尝试恢复。

通过标准：保存/重开前后持久化语义与采样结果一致；save-as 后 undo/redo 正确；文件失败不部分替换；已有保存冲突检测及发布后 warning 语义不变。MOC3 兼容性按实际 validator 分类，structural pass 不冒充官方运行验收。

### S3：Python SDK 与 CPU harness

新增 `modules/kasane-python`，交付 wheel、类型提示、异常和 `python -m kasane run`。Python fixture 与 Rust fixture使用同一输入数据和预期结果。完整接口 coverage 自动检查公开方法和测试映射，避免只绑定演示脚本用到的少数方法。

通过标准：外部 venv 中完成无 GPU 的查询、建模、绑定、参数采样、save/reopen；异常行号和业务错误字段可读；with edit 内错误回滚、块外已有提交保留；两线程交叉访问返回预定结果，无死锁/借用 panic；Python 快照数组修改不影响文档。

### S4：wgpu 观察 harness

新增 `modules/kasane-sdk-observe`，接入 Python 可选 feature。实现不可变观察输入、资源检查、GPU lifecycle、图像读回、裁剪、参数拼图和报告。

通过标准：直接观察未保存的模型；同一 Observer 连续观察参数变化、几何变化和纹理变化，图片及资源版本正确变化；仅 view 变化遵循 renderer 已有缓存契约；错误不能复用旧图冒充成功；采样不改变会话 preview；裁剪保留合成关系。没有窗口也能验收，但必须实际运行 GPU。

### S5：真实 agent 流程、发布与交付

新增 `tools/validate_sdk.py`、recipes 和 `SDK-ACCEPTANCE.md`。将验收命令、输入清单和图像/数值证据保留到 `target/sdk-acceptance/<run-id>`。

至少执行三条端到端流程：

1. 从两组不同尺寸/位置的 PNG 创建 mesh、part、Rotation/Warp、参数及 keyforms；检查中间插值；保存、重开、导出。
2. 从本地提供的真实外部 model3/MOC3 导入；读取原对象 ID；局部修改几何、父关系、绑定和绘制属性；保持未修改内容；保存和运行对照。
3. 第一段脚本生成作品和 ID 清单；第二段脚本检查报告和图像，指出一个具体问题并修改已有对象；保存修改前后证据。不能每次清空工程重建，也不能以固定“自动成功”脚本替代这条 agent 自检。

通过标准：wheel 在仓库外运行以上适用流程；必需测试均有证据。缺少 GPU、外部模型或官方 Core 的项目标 `not_run`，相应完整交付门禁不通过；可另行报告已经通过的 CPU/Rust 阶段。

## 11. 验收矩阵与命令

### 11.1 必需测试矩阵

| 类别 | 必需用例 | 证据 |
| --- | --- | --- |
| 领域覆盖 | 表 7 全部对象 create/read/replace/delete 或明确不支持的操作；字段与类型失败 | 方法 coverage + Rust/Python 结果 |
| 完整性 | UUID、重复 ID、非有限值、越界顶点、引用环、悬空引用、不完整 keyform | 错误码/目标/失败前后内容 |
| 编辑语义 | 第 N 步失败、catch 后 commit、drop/cancel、no-op、嵌套、过期版本 | revision/history/event 对照 |
| 生命周期 | 不同 session、换工程、删后同 ID 重建、undo/redo、候选句柄逃逸 | `STALE_HANDLE` 等实际结果 |
| 拓扑 | 稳定 vertex ID、替换 topology、绑定/glue 迁移或拒绝 | 按 ID 对齐的数据对照 |
| 历史和 IO | 全对象撤销、dirty baseline、save/save-as、资源 A/B、冲突、写入失败 | 文件 hash + reopen + undo/redo |
| 采样 | 默认、min/max、每个 key、中点；非对称 3×3；2×2×2 端点及中心；repeat | requested/actual + 数值报告 |
| 空间 | 非零 origin/ppu、Rotation→Warp、Warp→Rotation、反射、UV 非对称图 | 源/最终坐标与图像 |
| GPU | clean/crop、mask/offscreen、透明度/顺序、连续参数/资源/view 变化 | PNG、diff、adapter、资源 hash |
| 分发 | 外部 venv、无仓库 PYTHONPATH、无 Godot、CPU 无 GPU 初始化 | wheel hash + 命令/日志 |
| agent 创作 | 新建、导入编辑、第二脚本局部修正 | 脚本、报告、前后图和项目 |

数值与图像门槛沿用 [统一验收规则](VALIDATION.md)：离散字段精确一致；浮点 `1e-5 + 1e-5 * max(abs(a), abs(b))`，位置换算后最大 0.05 原画像素；确定区域 RGBA 各通道误差不超过 `2/255`，全图 MAE 不超过 0.005，大于 0.05 的像素占比不超过 1%。对象 crop 也需对照，不能用大片背景稀释差异。

该文档中的历史 Godot-only 宿主要求不用于新 SDK 运行环境；本文要求在固定 wgpu backend/profile/view 上验收，旧 Godot 结果仅作为现有对照来源。原文关于直接脚本保留已完成写入的要求在这里细化为：已提交批次保留，当前显式 edit 全部撤回。MOC3 官方/Purism 对照和缺失依赖不得当作通过的要求继续适用。

### 11.2 当前可运行的基线命令

仅第 2.1 节两条命令在本次实际运行。已有 renderer 回归入口可在实施触及观察边界时按 [RENDER-WGPU-DESIGN](RENDER-WGPU-DESIGN.md) 运行，不需要为纯文档提交重复验收所有后端。

### 11.3 SDK 验收入口

第一条 Rust 测试命令现已可运行；其余命令仍是交付要求，尚不存在：

```sh
cargo test -p kasane-sdk --locked
cargo test -p kasane-sdk-observe --locked
python3 tools/validate_sdk.py --profile cpu --output target/sdk-acceptance/cpu
python3 tools/validate_sdk.py --profile full --output target/sdk-acceptance/full
```

`validate_sdk.py` 负责构建/安装指定 wheel、调用 Python 测试、运行 recipes 并汇总证据。CPU profile 清楚列出范围；full profile 包含 GPU、外部模型和导出运行对照。外部资源由本地配置文件提供并记录 hash，不在命令中硬编码个人目录。

报告逐项使用 `passed/failed/not_run`，由检查结果生成。必需项失败或未执行返回非零；导入过程的 diagnostics 和发布成功的 warnings 独立保留。可复用 [acceptance_evidence.py](../tools/acceptance_evidence.py) 的检查汇总思想和兼容的实现部分，不手工填入整体 passed。

## 12. 性能、风险与后续边界

先测量以下工作负载：1 万/10 万顶点的批量更新；两轴 keyform 表构建；真实导入模型的 checkpoint 与 undo/redo；相同参数重复求值；连续 100 次参数变化和观察。记录 release 构建、机器、输入 hash、冷/热启动、p50/p95、峰值 RSS、checkpoint 估算字节、求值次数、GPU 上传/分配统计。

首版确定性门槛是：一个显式 edit 只创建一次 candidate；一次大数组写回只有一次语言边界调用；稳定 preview 复用既有缓存；metadata 修改不强迫 GPU 几何重建；相机变化遵循 renderer 已验收的零几何上传契约。绝对耗时门槛由 S0 实测后写入验收基线，本提案不虚构“毫秒级”承诺。

| 风险 | 实施处置 |
| --- | --- |
| 全内容 checkpoint 比旧 delta 消耗更多内存 | 设置预算、记录实际峰值；首先鼓励批量 edit；测得瓶颈后再做字段/对象 delta，不改变公开事务契约 |
| 保存路径改写破坏 undo/dirty | S2 资源重定位为阻断项；失败时不能静默清空历史或恢复旧 manifest |
| 绕过 SDK 写入漏记历史 | 不公开 mutable core/session；所有 wrapper 走统一发布入口；coverage 检查写入路径 |
| Python 生命周期或锁死 | ID 句柄、短时锁、edit token；无裸内部引用；跨线程和异常退出测试 |
| 完整验证太慢 | 区分单次 core 验证、批次最终结构检查和显式全采样诊断；不在每个顶点更新中重复全模型渲染 |
| 依赖环境误判 | capabilities 与实际执行结果分开；固定首个 wheel 平台；MSRV、GPU、官方 Core 独立验收 |
| 观察截图与模型版本不一致 | 不可变输入、同字节 hash/upload、完成后发布 report；记录捕获版本而非完成时当前版本 |
| 自动辅助过度扩张 | v0 仅矩形/规则网格和完整 keyform builder；自动网格、逆变形、自动 rig 另立需求 |

后续 UI 应通过 SDK 读取对象、执行 edit、订阅事件并消费帧；选择和相机属于 UI/观察状态。远程 agent 服务若出现真实需求，可在稳定的 session/version/edit 协议上增加适配器。当前不提前引入通用命令总线、插件运行时或自定义反射系统。

## 13. 首个实施任务的明确边界

从 S0 和 S1 的最小纵切开始：新增 crate；新建文档；导入一张 PNG 的描述；创建一个矩形 mesh；查询与批量位置更新；候选提交/回滚；结构编辑 undo/redo；CPU 求值。这个纵切只证明会话和编辑内核成立，不作为全部 bridge 能力完成。

2026-09-23 首轮实施记录：新增 `kasane-sdk`、core 内容 checkpoint、project 受限发布入口、非对称 PNG fixture 与三项公开 API 契约测试。首轮 `cargo test -p kasane-sdk --locked` 三项通过；core/project 原 50 项指定基线通过；`cargo clippy -p kasane-sdk --locked --all-targets -- -D warnings` 通过。当时 S1 的句柄和字节预算尚未完成。

2026-09-23 续实施记录：已增加 capacity 感知的全内容 checkpoint 估算、可配置历史预算、提交前超限拒绝、mesh/asset 显式替换、拓扑映射、参数与完整 mesh binding、稳定对象句柄，以及纯 mesh 名称编辑保留求值 revision。SDK 15 项契约测试、core 容量测试及原 50 项基线通过；Clippy 无警告。尚待 S1c 其它变形/Part/SceneBinding、S1d/S1e、S0 性能和 Python 兼容性验收。

2026-09-23 再续实施记录：已接入 Part、Rotation/Warp Transform、三种 SceneBinding track 的完整读写与 keyform 更新、rotation/warp 字段更新、组织/变形父节点设置，以及会话 `PreviewState` 的帧缓存和原子预览值更新。新增 SDK 场景契约测试覆盖层级环拒绝、完整形态表、失败批次回滚、预览缓存与独立求值；S1d/S1e、S0 性能和 Python 兼容性验收仍待实施。

2026-09-23 同轮补充：canvas 和显式 draw-order groups 已接入受控读写，`references_to` 和 `erase_object` 已接入。契约测试覆盖组合提交、无效组回滚、引用拒绝、跨批次删除及同 ID 重建后的句柄过期。SDK 当前 21 项测试通过；S1d 其余 BlendShape/Glue/Offscreen 对象族与 S1e 的完整诊断仍待实施。

2026-09-23 同轮再补充：BlendShape key table/constraint/binding、Glue 和 Offscreen 已接入同一候选提交路径，提供读写和句柄；Part binding 与 Offscreen 映射的联合替换入口也已接入。新增效果对象契约测试覆盖完整组合、无效约束回滚、联合扩容与 undo/redo。SDK 当前 23 项测试通过；S1e 全量诊断、S0 性能与 Python 兼容性、后续 S2–S5 仍待实施。

2026-09-23 提交 `b7e6d5a` 后续实施：修复同批次删除并以同 ID、同内容重建被误判 no-op 的问题；身份变化现会发布 revision、记录历史，且 undo/redo 不恢复旧句柄。增加 core `validate_structure` 与 SDK 提交前检查，校验全部持久对象集合、顺序、关系与 core 字段规则，checkpoint 恢复路径由单元测试覆盖。增加独立的源几何 warning 诊断。SDK 当前 25 项契约测试通过；[初步合成模型测量](SDK-PERFORMANCE.md)记录 8/512 mesh 的内容估算与候选/校验时间。S2 工程资源闭环、Python 兼容性、真实大模型性能、S3–S5 仍待实施。

2026-09-23 提交 `a370546` 后续实施：增加 `AuthoringSession::new_project`，以 expected version 检查后原子替换内存文档；成功推进 generation、清理旧历史/预览/事件并使旧句柄失效，失败保留原状态。SDK 当前 26 项契约测试通过。保存、打开与跨保存历史仍在 S2 范围。

2026-09-23 续实施记录：S2 已接入 `open_project`、`save_project`（同路径保存与另存为）、`project_path` 和 `diagnose_resources`。打开时先校验完整结构，失败保持旧会话；成功推进 generation。保存沿用 project 的发布和冲突检测，成功后按资源 ID、解析后的旧路径、尺寸及 hash 重定位 done/redo checkpoint；不匹配的历史相对资源改为原工程根目录下的绝对路径。SDK 工程测试覆盖空 hash 旧资源、A→B 同 ID 替换、已删除的历史资源、redo 分支、失败保存/打开，以及重开后的持久对象与 CPU 求值一致。model3/MOC3 导入、导出和显式资源 relocate 仍待 S2 后续。

2026-09-23 同轮后续：S2 增加 model3 与 bare MOC3 的原子导入包装、`ImportReceipt`、绝对纹理槽位映射校验，以及不改变会话版本/历史的 `export_package`。资源准备辅助 `prepare_relocated_asset` 验证同内容换路径；替换内容仍使用 `prepare_png_asset` 和 edit。新增 SDK 工程测试覆盖外部 v5 fixture 的导入、编辑、保存、导出，bare MOC3 槽位路径失败，以及 relocate 与 replace 的不同 hash 语义。独立预检查入口、发布后 warning 的 SDK 注入测试和 S2 汇总验收仍需补齐。

2026-09-23 同轮补充：SDK `with_filesystem` 提供可注入的发布后端，`new_project` 保留该后端。发布后目录同步失败的 SDK 契约测试确认：返回 `durable=false` 与 warning，仍更新保存基线、保留跨保存 undo/redo，随后同路径重试不触发冲突。独立外部模型预检查入口与 S2 汇总验收仍待补齐。

2026-09-23 同轮路径契约修正：`prepare_png_asset` 现要求绝对路径；相对用户路径通过 `prepare_png_asset_from_base` 显式传入绝对 base，避免按进程 cwd 解析。SDK 测试覆盖错误路径与正确解析。

2026-09-23 S3 首批实施：新增 `kasane-python` mixed-layout wheel，固定 PyO3 0.29.2 与 maturin 1.15.0；原生 Session 使用短时锁，`with edit` 收集命令后一次调用 Rust SDK 发布。已接入 PNG/矩形建模、参数和完整 mesh binding、位置更新、CPU 求值、工程导入/保存/导出、结构化业务异常，以及一次性 runner 的 stdout/stderr、异常行号与会话版本报告。仓库外 CPython 3.14 venv 的 7 项测试与 CPU recipe 通过；公开入口覆盖检查记录 29/115 已绑定，剩余 86 项及 GPU 观察仍待实施。

2026-09-23 S3 同轮后续：补工程重置、asset/mesh/parameter/几何快照、全部对象 ID 列表、名称与引用查询、结构诊断、历史状态和事件、独立预览值与帧。Python 快照的版本与数据在同一次原生锁内读取；预览失败不覆盖旧值。仓库外 wheel 的 9 项 Python CPU 测试通过；公开入口覆盖升至 59/115，剩余 56 项待绑定。

2026-09-23 S3 继续：补 canvas/parameter 替换、删除对象、层级关系编辑与 mesh binding 查询。参数替换保留现有 runtime ID 等未显式提供的字段。仓库外 wheel 的 11 项 CPU 测试通过，覆盖为 69/115，剩余 46 项；层级关系编辑成功路径随 Part/Transform 绑定继续验证。

2026-09-23 S3 再续：接入不透明对象句柄、绘制顺序组读写和自定义历史限制。句柄在改名后可解析，undo/redo 后旧对象身份不再有效。仓库外 wheel 的 13 项 CPU 测试通过，公开入口覆盖为 77/115，剩余 38 项待绑定。

2026-09-23 S3 Part 批次：接入 Part 创建、读取和替换，同批次验证组织父节点及 mesh Part 归属。仓库外 wheel 的 14 项 CPU 测试通过，公开入口覆盖为 80/115，剩余 35 项。

2026-09-23 S3 Transform 批次：接入旋转与 warp Transform 创建、读取和数据更新，并以真实层级验证 Transform 父节点/Part 归属及 mesh 变形父节点。仓库外 wheel 的 15 项 CPU 测试通过，公开入口覆盖为 84/115，剩余 31 项。

2026-09-23 S3 PNG 资源批次：接入显式基目录导入、内容替换与相同内容搬迁。仓库外 wheel 的 16 项 CPU 测试通过，公开入口覆盖为 87/115，剩余 28 项。

2026-09-23 S3 覆盖审计续批：修复检查器漏扫独立 `diagnostics.rs` 的问题，接入 Python 几何提示查询；将仅用于 Rust 文件系统故障注入的 `with_filesystem` 明确记录为 Rust 专用并关联测试。当前清单为 116 个公开入口：88 个 Python 绑定、1 个 Rust 专用、27 个待绑定。仓库外 wheel 的 17 项 CPU 测试通过。

2026-09-23 S3 SceneBinding 批次：Python 现支持 Part、Rotation、Warp 三种轨道的完整 keyform 创建、读取、替换和单项更新，保留 appearance 与 rotation pose；测试覆盖快照副本、失败回滚和保存重开。仓库外 wheel 的 18 项 CPU 测试通过，清单为 93/116 Python 绑定、1 项 Rust 专用、22 项待绑定。

2026-09-23 S3 mesh binding 批次：keyform 的 appearance 与 draw order 不再在 Python 桥接中丢失；支持完整 binding 替换与单 keyform 更新。仓库外 wheel 的 19 项 CPU 测试通过，清单为 95/116 Python 绑定、1 项 Rust 专用、20 项待绑定。

2026-09-23 S3 mesh 绘制属性批次：Python 支持读取和更新纹理引用、外观、绘制顺序、混合模式、启用状态、双面、反转遮罩与遮罩列表；更新保留 mesh 几何和身份，非法混合模式使整批 edit 回滚。仓库外 wheel 的 20 项 CPU 测试通过，清单为 96/116 Python 绑定、1 项 Rust 专用、19 项待绑定。

2026-09-23 S3 Transform 替换批次：快照补全 appearance，Python 可用完整快照一次替换 rotation/warp Transform 的名称、层级、数据、启用状态与外观，并保留 runtime ID。保存重开、失败回滚及 undo 通过仓库外 wheel 的 21 项 CPU 测试；清单为 97/116 Python 绑定、1 项 Rust 专用、18 项待绑定。

2026-09-23 S3 Offscreen 批次：Python 支持 Offscreen 创建、读取和替换，完整保留 keyform 的 opacity/multiply/screen、Part 槽映射及 runtime ID；Part scene binding 与 Offscreen 映射可在单次 core 操作中联合扩容。仓库外 wheel 的 23 项 CPU 测试通过；清单为 101/116 Python 绑定、1 项 Rust 专用、14 项待绑定。

2026-09-23 S3 Glue 批次：Python 支持 Glue 创建、读取和替换，包括顶点对权重、静态强度和完整参数绑定表；替换保留 runtime ID。快照副本、保存重开、无效顶点回滚及 undo 通过仓库外 wheel 的 24 项 CPU 测试；清单为 104/116 Python 绑定、1 项 Rust 专用、11 项待绑定。

2026-09-23 S3 BlendShape 元数据批次：参数创建/替换可指定 `blend_shape` kind，快照包含 kind；Python 支持 key table 与 constraint 的创建、读取和替换。快照副本、保存重开、无效长度回滚及 undo 通过仓库外 wheel 的 25 项 CPU 测试；清单为 110/116 Python 绑定、1 项 Rust 专用、5 项待绑定。

2026-09-23 S3 BlendShape binding 批次：Python 对 mesh、warp、rotation、Part、Glue、Offscreen 六种 target 暴露有类型的 delta keyform，支持创建、读取与完整替换。六类型字段往返、快照副本、保存重开、失败回滚和 undo 通过仓库外 wheel 的 26 项 CPU 测试；清单为 113/116 Python 绑定、1 项 Rust 专用、2 项待绑定。

随后按第 7 节补齐对象族，并推进 S2 的跨保存历史。Python 薄绑定和观察宿主分别在对应契约稳定后接入。每次阶段报告明确已实现接口、实际运行的验收、未完成项以及下一阶段入口。
