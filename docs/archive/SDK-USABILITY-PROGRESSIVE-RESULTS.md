# SDK 渐进可用性实验结果

协议、评分与梯度见 [渐进实验](SDK-USABILITY-PROGRESSIVE.md)。以下为探索性工程证据；只有两位 Luna 受试者，且 S1 延续 S0 的使用经验，不能把成绩解释为陌生人群的成功率。

环境：Git 基线 `1a9d4d095a7ee9502a40c53c197d5f348901e2f6`，CPython 3.14.3，`uv` 0.10.7，macOS arm64；`kasane` 0.1.0 CPU wheel SHA-256 `294a7b8cc6e07ff3a0f5259d5b7c4be045d481bed042705fe5c4d1078fe81283`。wheel 由 `RUSTFLAGS='-C strip=none' uv build --wheel` 构建，使用 `uv venv` 与 `uv pip install` 安装至隔离环境。原始任务包、受试脚本、笔记和逐项判分 JSON 均在本机 `target/sdk-usability/progressive/`，不纳入 Git。

## S0：PNG 新建工程

正控制 14/14 通过；故意把 mesh 改名的负控制失败。两位受试者初次结果和从新空目录的复跑结果均为 **14/14 通过**，Pyright 各 0 错误，输入素材及文档 hash 未变；人工审阅脚本未见直接改 JSON 或私有原生调用。按预设口径均为 **100/100**。自报耗时分别约 5、3 分钟，未自动记录完整命令轨迹。

两人分别把画布快照原点误认为 `origin`。一人在执行时遇到 `AttributeError`，另一人由 Pyright 在执行前发现；两人自行改用 `origin_x`/`origin_y`。任务命令 `uv run --no-project` 还给出无项目警告，虽不影响执行但干扰诊断。随后在公开 [Python 类型速查](SDK-PYTHON-TYPING.md) 中明确画布字段，并把后续任务命令改为 `uv run --python ... python ...`。最初未加 `RUSTFLAGS` 的 wheel 在本机报原生模块 LINKEDIT 加载错误；这发生在受试者实验前，已由构建门禁拦截并修复，未计入成绩。

## S1：添加参数与关键形态

正控制 **21/21** 通过，故意把终点 X 少移 1 像素的负控制在关键形态和 0.5/1.0 求值项失败。两位受试者作品经独立判分均 **21/21 通过**；从新空目录复跑后仍各 21/21，Pyright 各 0 错误，输入工程文件 hash 未变；人工审阅脚本符合公开 API 约束。两位均为 **100/100**，自报耗时约 5、4 分钟。

受试者 A 第一次把参数显示名 `open` 当成 `evaluate` 的映射键，收到 SDK 失败后自行改为参数 ID。已在公开类型速查中明确该键的身份和坐标转换。

主持人的首版 S1 判分器只接受 `output/` 子目录，但任务仅要求结果在新空目录；A 把修正后的作品保存到包内 `output-retry/`，被隐藏路径约束拒绝。这是 harness 缺陷，不能算 SDK/受试者失败。判分器改为允许任务包内、输入工程外的独立结果目录；修复后的正负控制仍符合预期，并对两份**原始**作品重新判分。语义检查没有放宽。原始拒绝报告与修复后报告均保留。S1 使用同一两位 S0 受试者，成绩包含梯度学习效应。

## S2：局部修改双 mesh 工程

复用现有局部编辑任务与独立判分器；正控制 **36/36** 通过，未作修改直接另存的负控制在目标终点关键形态和终点求值项失败。两位 Luna 的原始工程、全新目录复跑均各 **36/36** 通过，Pyright 各 0 错误，输入工程 hash 未变；人工审阅脚本未见直接改 JSON 或私有原生调用。两位均为 **100/100**，自报耗时约 8、7 分钟。

两人独立在保存重开后的坐标精确比较中遇到浮点舍入差异，改用 `1e-6` 绝对容差后通过。一人还由 Pyright 发现其自定义断言函数不能缩窄可选快照类型。公开类型速查已补充保存后坐标的比较口径。这轮继续使用同两位受试者；结果说明现有任务在熟悉 SDK 的渐进流程中可完成，不等于全新使用者的独立成功率。

## S3：外部 model3 导入与导出

正控制 **66/66** 通过，故意重命名 mesh 再保存及导出的负控制在内容项失败。两位 Luna 的原始输出和全新目录复跑均各 **66/66** 通过，Pyright 各 0 错误，输入 model3、MOC3、纹理和文档 hash 未变；两位均为 **100/100**。自报耗时约 4、6 分钟。

一位受试者初次逐字段比较 asset 时发现保存会把 `source` 重定位到工程资源目录；他自行改为比较 ID、名称、尺寸和 SHA-256，并运行资源诊断。此轮没有改 SDK；后续改版优先解决重复保存、参数名求值和画布原点的高频阻碍。

## Python SDK 改版复测

根据 S0、S1 与历史局部编辑实验的障碍，直接修改 `kasane` Python SDK：

- `CanvasSnapshot.origin` 返回 `(x, y)`，与 `Session(..., origin=...)` 的输入形状一致。
- `Session.parameter_id(name_or_id)`、求值、完整求值、预览和 Observer 允许唯一参数显示名；重名明确拒绝并要求 ID，原有 ID 仍可用。
- `Session.save(path, on_exists="new")` 在目标属于其他工程时选择编号新路径，返回实际 manifest；默认仍拒绝覆盖，当前工程被外部修改时仍报 `PROJECT_CONFLICT`。

新 CPU wheel SHA-256 为 `7b842c65003329e254d863739a0e2d924d35c58cf3f5ce3ddf9df1b8ecb7522e`，由同一 CPython 3.14/uv 环境构建并隔离安装。仓库外运行已安装 wheel 的 Python CPU 套件 **35/35** 通过；新增检查覆盖显示名、重名、重复键、原点、编号保存及冲突保持。

两位**新的** Luna 受试者使用新 wheel、独立任务包，均实际从两个独立 Session 向相同目标请求保存，并分别通过 `canvas.origin` 和 `{"open": 0.5}` 完成报告。正控制 **21/21** 通过，错误原点的负控制失败。两位原始结果和全新目录复跑均各 **21/21** 通过，Pyright 各 0 错误，输入工程 hash 未变；脚本审查确认两人均调用 `save(..., on_exists="new")`，并使用 SDK 返回的两个不同 manifest。两位按口径均为 **100/100**，自报耗时约 3、5 分钟。

一位受试者先误猜 `Evaluation.meshes`，从公开对象信息改用 `Evaluation.drawables`；另一位无阻碍。这个错误未触发新的 SDK 别名：求值输出的 drawable 不一定等同于可编辑 mesh，别名会混淆语义。此轮与旧轮任务及文档不同，只证明新增接口在该任务中可发现且可运行，不构成因果 A/B 结论。受试者时间仍为自报，没有自动命令轨迹。

## S4：观察渲染并修正

使用带 `observe` feature 的 wheel，SHA-256 为 `44a65cc3b710e2997b7cea688809f2be665d8629cb67c74fcc4cac18f31`。参考图由独立工程生成，受试者只知道相同 Observer 设置，不知道隐藏的顶点修正量。正控制 **18/18** 通过；未修正关键形态的负控制在目标形态、参考图和报告图像项失败。

两位 Luna 都把 `badge` 的 `open=1` 关键形态沿源画布 X 方向修正 10，保存重开后采样 `open=0/0.5/1`，交付帧图、contact sheet 和报告。原始结果及全新输出目录复跑均各 **18/18** 通过，Pyright 各 0 错误，输入工程和参考图 hash 未变；脚本审查确认只通过公开 SDK 修改工程。两位均为 **100/100**，自报耗时约 8、7 分钟。

两人独立尝试用 Pillow 比较 PNG，但隔离 wheel 环境没有 `PIL`。一人自己写标准库 PNG 解码后验证零差异，另一人依靠图像观察和报告 hash。另有人误猜 `observe_run` 的报告直接位于指定输出目录，实际在唯一运行子目录；使用返回的 `ObservationRun.report` 后解决。前一障碍明确属于 SDK 缺少图像诊断入口，因此后续直接在 SDK 增加 `compare_png` 和 `ObservedFrame.compare_png`，再以新 wheel 重测同一关。图像对照 API 的效果以重测结果单独记录。

## S4 SDK 图像诊断改版

SDK 新增无外部图像依赖的 `kasane.compare_png`、`ObservedFrame.compare_png` 和 `ImageComparison`，可读 RGB8/RGBA8 PNG，报告像素差、变化范围并保存差异图。observe wheel v3 SHA-256 为 `2e82a647b53f696fe9f337dea7a979ca371c88b1a2c37cc662efc80bd0cd2860`。已安装 wheel 在仓库外运行 CPU **37/37**、GPU **3/3** 通过；正控制 **18/18**，负控制在目标形态及图像项失败。

两位新 Luna 均实际使用 SDK 图像比较求得最终 `changed_pixels=0`；原始作品及全新目录重跑均各 **18/18**，最终 Pyright 各 0 错误，按口径各 **100/100**。两人独立发现：包级 `kasane.compare_png` 运行时可用，但 Pyright 认为它没有公开导出；一人改用 `ObservedFrame.compare_png`，另一人通过动态 `getattr` 绕过类型检查。后一做法是 SDK 类型表面缺陷的证据，不应作为建议用法。另有人误猜 `ObservationRun.output`，实际只有 `directory`。

一位受试者按任务要求把结果写在 `/tmp`，首版 S4 判分器却暗含任务包 `output/` 路径要求。修复为检查任意绝对输出根、并要求 manifest 和报告位于同一根下后，保留原始作品重判；正负控制仍符合预期。该问题计入 harness，不计入 SDK 或受试者成绩。v3 由此仍发现 SDK 缺陷，不作为最终稳定版本。

SDK 随后把 `compare_png` 和 `ImageComparison` 明确加入公开导出，新增 `ObservationRun.output` 别名，并使差异 PNG 的变化像素可见。observe wheel v4 SHA-256 为 `e202cf55fac2ee1222e878e1bc6d4cdff42f14268f96afb7c9b211a351f1b6d0`。隔离安装后 CPU **37/37**、GPU **3/3**、直接引用包级 API 的 Pyright **0 错误**；新正控制通过、负控制失败。v4 的两位新受试者复测另行评分，不和 v3 成绩合并。

v4 两位新 Luna 的原始作品与全新目录复跑均各 **18/18**，Pyright 均 0 错误，按口径各 **100/100**。两人均用公开 `kasane.compare_png` 核实终点渲染 `changed_pixels=0`。一人把 SDK 的观察报告复制到输出根并添加比较指标，原始判分器只在报告同目录寻找帧和 contact sheet，故初判 16/18。任务未规定报告布局；判分器改为以 summary 的绝对路径和报告中的帧 SHA-256 核验实际文件，原始作品重判为 18/18，正负控制保持区分。两人还分别把 `frame.png` 字节及字符串路径传入只接受 `Path` 的比较入口，收到运行时错误后自行修正；这是同一类型的易用性摩擦。

因此 SDK v5 将 `compare_png` 的两个输入及 `ObservedFrame.compare_png` 的参考输入扩为 `Path | str | bytes`，其中 bytes 为 PNG 内容。observe wheel SHA-256 为 `21344f0789a39cbac54dd20798b29f7053f9b8337534bdae49d4a7d57ca973ac`；隔离安装后 CPU **37/37**、GPU **3/3**、Pyright **0 错误**，正控制通过、负控制失败。v5 另由两位新 Luna 复测，不与 v4 成绩合并。

v5 两位 Luna 的原始作品、全新目录复跑均各 **18/18**，Pyright 各 0 错误，输入工程、参考图与文档 hash 未变；脚本审查确认使用公开 SDK 编辑与观察。两位均为 **100/100**，终点图与参考图逐像素相同。受试者 A 的一次相对工程路径尝试收到清晰的绝对路径错误后修正；受试者 B 起初猜错 `observe_run` 的随机子目录，随后使用返回的 `ObservationRun.report`。两人最终都没有阻断性 SDK 问题，S4 按预设门槛稳定。

同一源码还构建了默认 CPU wheel（SHA-256 `9f7cb1c6e702274f43e2e5e2b9c8e60719f285eb81f561bcd4efdbbb5c4c9776`），在独立 `uv` 环境、仓库外运行 CPU **37/37**，并对包级图像 API 做 Pyright 检查，结果 0 错误。图像比较不依赖 GPU feature；只有实际观察渲染需要 observe wheel。

100 分衡量**最终交付、重放、静态类型和记录完整性**，不代表首次尝试零错误。早期 SDK 缺陷、临时绕法、路径猜测和判分器误判均逐项保留在受试笔记及本记录中；样本量、前期关卡经验和自报耗时限制了对一般开发者成功率的外推。
