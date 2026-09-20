# M4 本机验收记录

2026-09-20，在 macOS arm64 / Apple M4 / Godot 4.7.2 / OpenGL Compatibility 下，构建 `kasane-godot` release 库后运行 `python3 tools/validate_m4.py --library target/release/libkasane_godot.dylib`，汇总报告为 `target/kasane/m4/report.json`。

| 门禁 | 结果 | 证据 |
|---|---|---|
| 真实 GPU 对照 | 84/84 通过；三幅全图均值误差与最大误差均为 0；解析像素检查最大通道误差约 0.002048，低于 2/255 | `target/kasane/m4/gpu/report.json` |
| Godot 集成 | 140/140 通过，含固定拓扑资源复用、拓扑和纹理更换、120 次连续顶点更新、覆盖层隔离及 50 轮打开/关闭 | `target/kasane/m4/godot/report.json` |

Rust 绘制核心可直接接受 `DrawableFrame` 与纹理表。M6 的 PurismCore C99 FFI、运行帧转换及 Viewer 接入属于 M6 验收，未包含在上述通过数中。`gd-cubism` 保持独立，继续作为 GPU 对照基准。

## Review 修复后复验

2026-09-20，同一平台重新构建 release 库，运行 `python3 tools/validate_m4.py --library target/release/libkasane_godot.dylib --output-dir target/kasane/m4-fixes`，汇总报告为 `target/kasane/m4-fixes/report.json`。

| 门禁 | 结果 | 证据 |
|---|---|---|
| Rust 帧校验 | 6/6 通过；覆盖不完整三角形、越界索引、UV 数量、重复 ID、悬空遮罩、非有限值和坐标转换溢出 | `target/kasane/m4-fixes/rust.log` |
| 真实 GPU 对照 | 84/84 通过 | `target/kasane/m4-fixes/gpu/report.json` |
| GPU 渲染回归 | 12/12 通过；覆盖重新入树、禁用与条件更新视口、遮罩两次单次更新、覆盖层刷新像素一致性及移除后恢复干净画面 | 同一报告的 `renderer_checks` |
| Godot 集成 | 142/142 通过；新增模型容器与覆盖层排序检查 | `target/kasane/m4-fixes/godot/report.json` |

观察接口的显式视口更新要求见 [M4 观察一致性](M4-shared-renderer.md#4-观察一致性)。本次复验未增加长期 RSS/GPU 内存压力测试，原有 120 次同步更新检查仍仅证明该用例内的静态内存增量受限。
