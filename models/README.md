# Live2D 模型目录

这里记录仓库实际使用的完整 Live2D 模型。`local/` 是唯一的本地模型源目录，已被 Git 忽略；不要提交受第三方许可限制的模型文件。小型、由本项目生成的 MOC3 回归夹具继续放在 `tests/fixtures/`。

| 模型 | 模型源位置 | 使用处 |
| --- | --- | --- |
| Nijiiro Mao (`mao_pro`) | `models/local/mao/runtime/mao_pro.model3.json` | `benchmarks/cubism-matrix` 的六种对照运行，以及可选的 MOC3 导入、导出测试与示例。 |
| Hiyori | `third_party/CubismSdkForNative-5-r.5/Samples/Resources/Hiyori/` | 可选 MOC3 导入测试。 |
| Rice、Mark | 同一 SDK 的 `Samples/Resources/Rice/`、`Samples/Resources/Mark/` | 可选旧版本 MOC3 导入测试。 |
| Ren | 同一 SDK 的 `Samples/Resources/Ren/` | 可选 MOC3 导入与导出测试。 |

将 Mao 的完整模型包放到 `models/local/mao/`，保留 `runtime/` 内的相对路径。运行 `python3 benchmarks/cubism-matrix/tools/matrix.py prepare-godot` 或构建渲染 benchmark 时，工具会把模型复制到 benchmark 工程的已忽略 `assets/live2d/mao/`；该目录是生成副本，不作为模型源。Core-only benchmark 与 MOC3 测试直接读取 `models/local/mao/`。

Cubism SDK 样本保留在 SDK 安装包原位，避免复制模型并破坏 SDK 自身的目录结构。缺少本地 SDK 或 Mao 时，相关可选测试会跳过；正式 benchmark 需要按 [`benchmarks/cubism-matrix/README.md`](../benchmarks/cubism-matrix/README.md) 安装依赖。

SDK 包还含有 Haru、Mao、Natori、Wanko 样本；仓库当前代码没有直接使用这些模型。这里的 Nijiiro Mao (`mao_pro`) 与 SDK 包中的 `Mao` 是不同的输入。
