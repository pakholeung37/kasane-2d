# 将 Shirousagi 的单图层美术修订应用到交付模型

`input/project/` 是从真实 Shirousagi 交付模型导入的可编辑工程，保留其 24 个
ArtMesh 和 34 个参数。`input/Shirousagi.psd` 是对应的 24 层美术源文件。
`input/revision.json` 指明本次修改的图层及 RGB 通道倍率；
`input/original-layer.png` 和 `input/revised-layer.png` 是从 PSD 对应图层得到的
修改前后美术样张。颜色规则是对每个非透明像素的 R、G、B 分别乘以给定倍率，
取 `floor(值 + 0.5)`，截断到 `[0,255]`；alpha 不变。

请把该**单图层**颜色修订应用到交付模型的纹理图集，使它在眨眼、转头和表情变化时
继续跟随现有 rig。保留其他图层的图集像素，以及全部网格、参数、关键形态、
变形器、层级关系和运行时 ID。不要修改输入工程、PSD 或输入 PNG。可使用 Pillow
处理纹理，工程修改只能通过公开 `kasane` API 完成；不要直接改写工程 JSON。

交付：

- `solution.py --output <绝对目录>`，从脚本所在目录寻找 `input/`，在新的空输出目录
  可重放。先在任务包的 `output/` 实际运行。
- 输出根内保存的可编辑工程、可搬迁 model3/MOC3/纹理 package。
- 输出根或交付子目录内的 `result.json`，包含实际绝对路径 `project_manifest` 和
  `package_model3`。如有多份，主持人读取最新写入的一份。
- `notes.md`：说明 PSD 图层到原模型图集的定位依据、纹理修改范围、多种参数姿态
  的验证、遇到的错误与恢复、依赖用途和仍有的不确定性。

不要读取主持人目录、源仓库或其他受试任务。主持人会重开并搬迁工程与 package，
检查修订前后的多个公开和未公开姿态、目标图层以外的像素、原 rig 状态和空目录重放。
