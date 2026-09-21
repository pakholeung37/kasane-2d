# M3B 审查后的实施补齐

日期：2026-09-21。审查修复基线：`21a91ee`。原始问题记录见 [M3B-REVIEW](M3B-REVIEW.md)。

## 实现

| 契约 | 本次补齐 |
|---|---|
| F01/F02 | model3 导入报告补 Groups、HitAreas、Layout 的未导入说明；参数类型在脚本与检查面板中可见 |
| F03–F05/F10 | table、constraint、四类 BlendShape binding 的结构化 Dictionary 读写/快照；对象树、参数关键值标记、约束/目标跳转、普通与增量绑定标签 |
| F06 | `GlueBinding`/`GlueKeyform` 由 Glue 持有，普通多轴强度网格按首轴最快排列；导入、插值、工程保存、MOC3 导出贯通；移除动画 Glue 临时拒绝 |
| F07 | `replace_mesh_topology` 联合校验 Mesh、普通形态、全部增量形态、Glue 与完整 VertexMapping。候选文档通过后一次 revision 提交，保留对象 ID 和集合顺序；失败不改变内容 |
| F08 | 工程写 v3，读 v1/v2/v3。v3 防止旧 v2 reader 静默丢失专用 Glue binding，并保留 root Rotation 的双精度编辑原点；旧 `binding_id` 伪 MeshBinding 引用明确拒绝 |
| F09 | Glue 完整关键形态与绑定写出；缺失 BS key table、constraint 或 Glue vertex 不再降级成索引 0 |
| F11 | 3,505 组依赖采样、独立打包应用、真实纹理脱离、逐类编辑/撤销/重做、双 Core 编辑导出、全图/前景局部 GPU 对照及独立合成 GPU 检查 |

正式脚本契约与字段见 [Editor API](../../apps/editor/API.md)。

## 精度修复

`ArtMesh174` 的失败路径包含多层 Rotation。父旋转角通过变换后的两个近点估计，平移项过早参加浮点累加会损失低位，误差沿嵌套链放大。

Rust 的三角/双线性插值明确使用融合乘加；Rotation 先完成线性部分 `m00*x + m01*y`，最后加 origin。PurismCore 的 `PurismDeformer.h` 同步采用这个顺序，而不是对验收阈值加例外。

修复前 ParamInkDrop=30 时官方与 Document 的首个失败点约 0.092822 像素；550 组回归修复后全部通过，最大约 0.045115 像素。PurismCore 原始 13 组基线的最大误差从超标降至 0.012705 像素。继续使用原始 `1e-5 + 1e-5*max(abs(a),abs(b))` 与额外 0.05 原画像素双门禁。

扩大到全最小值/全最大值后，还定位到 root Rotation 原点在“runtime f32 → canvas pixel f32 → runtime f32”中的不可逆舍入。`RotationPose.origin` 改为 `PreciseVec2`（f64 编辑坐标），工程、脚本读取和导出保留双精度；转换到运行域后仍用 f32 Core 算法。这不是保存原始 MOC3 的旁路缓存。两组极端姿势修复后，原文件→再导出最大误差约 0.000691 像素，Document→官方约 0.008383 像素。

Cargo 构建脚本同时跟踪 Purism 的 include/src，避免修改子模块头文件后 Rust 测试仍链接旧算法。

## 应用问题

- 实测发现预览每个 Mesh 都重新读取、解码同一张 4096² PNG。Mao 有 260 个 Mesh；现在一次 refresh 对每个唯一纹理只做一次读取/校验。打包应用实测参数更新从约 135 ms 降至 17.6–19.9 ms。纯参数/相机变化复用已验证纹理；显式 refresh、Document 修改与纹理替换仍检查资源。
- 全模型“适应画布”跳过不可见 Drawable，避免隐藏的远端顶点把角色缩成小图。指定单 Mesh 的定位保留原行为。
- 新增结构化 Dictionary 转换支持 GDScript 的 StringName 键、Vector2 与常见 packed arrays；数值保持整数/浮点类型，不要求脚本拼 JSON 文本。

## 可复现验收

本地需要原始 Mao、官方 Native SDK、Godot、现有 gd-cubism 官方 Core framework。

```sh
python3 tools/build_editor.py \
  --godot target/godot-tools/standard/Godot.app/Contents/MacOS/Godot \
  --export dist/editor/Kasane-Editor.zip \
  --template target/godot-tools/templates/macos.zip
python3 tools/validate_m3b.py --output-dir target/kasane/m3b-final
```

完整入口依次执行 Rust/双 Core 数值、原始 Core 基线、独立打包 Editor、合成 GPU 检查。任何必需项未运行或失败都不能得到 passed；子进程失败时清除旧成功报告，避免把历史结果误当成本次通过。

也可独立诊断各层：

```sh
python3 tools/validate_m3b.py --numerical-only --output-dir target/kasane/m3b-final
python3 tools/m3b_baseline.py --output-dir target/kasane/m3b-final
python3 tools/validate_m3b_editor.py --output-dir target/kasane/m3b-final/editor
python3 tools/validate_gpu.py \
  --godot target/godot-tools/standard/Godot.app/Contents/MacOS/Godot \
  --library target/release/libkasane_godot.dylib \
  --output-dir target/kasane/m3b-final/gpu
python3 tools/validate_m3b.py --collect-acceptance --output-dir target/kasane/m3b-final
```

`--collect-acceptance` 只汇总已存在的证据，不运行或补造检查；记录每份报告路径和 SHA，并检查原模型身份一致。`--numerical-only` 明确只代表数值子集。

打包应用在临时目录与独立 HOME、最小 PATH 下启动，复制 Mao 后导入/保存，再删除**复制的源包**并重开；不会移动或删除用户原素材。脚本覆盖四类增量形态、key table、constraint、动画 Glue、稳定顶点 ID 改写、Undo/Redo、失败写入原子性、Inspector 选择。编辑导出由两个 Core 驱动；A→B→A 图像保持一致，资源集合不增长。

## 最终验收结果

2026-09-21，macOS arm64，Godot 4.7.2、OpenGL compatibility，release GDExtension 与独立 export-debug 应用。总报告 `target/kasane/m3b-final/report.json` 为 **passed**，数值失败数为 0。证据在本地 target 下生成，不提交大体积模型、截图或探针快照。

| 检查 | 结果 |
|---|---|
| 3,505 组参数输入 × 官方/Purism 双 Core × 三条对照路径 | 全部通过；147 批、294 个 provider case，最大位置误差 **0.040447712 像素**，最大浮点误差 6.9737434e-6 |
| 原始模型双 Core 基线 | 13 组通过；最大位置误差 0.012704730 像素 |
| 打包 Editor | 独立 HOME/PATH 启动，真实导入、保存、移除源包副本、重开通过 |
| 编辑闭环 | 四类 BlendShape、表、约束、动画 Glue、联合拓扑、Undo/Redo、无效写入原子性、Inspector 与编辑后双 Core 导出通过 |
| 模型 GPU 对照 | 7 组 A→B→A，全图及 28 个前景/上中下区域通过；局部最大 MAE 0.000074594，超差像素比例 0.000078873 |
| 合成 GPU / Workspace | 84 / 10 项通过 |
| Rust / Python / Purism | Rust 70 项测试及 workspace all-targets 检查、Python 7 项、Purism 402 个断言通过 |

验收模型 MOC3 SHA-256：`247d028f9900be2a46e4530816ece5749524d35013ba77424bd943598e0e54ff`。打包产物 `dist/editor/Kasane-Editor.zip` SHA-256：`5475b2b8de172a68aec289806cfbd9fffe11257ba1427c986074282b569cafdc`。Purism 修复提交为 `ad51276`。报告保留测试时工作区 revision、dirty 状态、输入清单、引擎/构建配置和各证据 SHA；本次是在提交前的最终工作区运行验收。

## 保留的范围边界与后续建议

M3B 的验收模型是 Mao 5.0，普通 3.3 模型保留回归。BlendShape Glue、循环参数、Offscreen/新版本语义，以及同一目标的重复/共享 BlendShape group 仍显式拒绝；不能称为所有 MOC3 的通用支持。model3 的 Physics、Pose、Motions、Expressions、Groups、HitAreas、Layout 只报告为未导入附件，不纳入 Document 编辑语义。

后续可改进：按 revision 缓存目标到 BlendShape binding 的索引；进一步测量 renderer 的批量属性提交成本；用命名 schema 索引替换 decoder 中的裸 section 数字。这些应独立测量/验证，不改变本次模型语义或验收阈值。
