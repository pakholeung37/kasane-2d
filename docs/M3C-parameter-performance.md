# M3C 参数拖动卡顿定位

日期：2026-09-21。对象：官方 SDK 的 Mao、Ren，源码 Editor。

## 结论

本机显著卡顿的主要原因是源码 Editor 使用未优化的 Rust debug 原生库。S6 完成记录明确说明将 `apps/editor/native/libkasane_godot.dylib` 同步为 debug 库；常规 `tools/build_editor.py` 原本构建并安装 release 库。

参数滑块事件还存在五次整帧求值，放大了 debug 开销。这条重复求值路径在 M3C 前已存在，不能全部归因于新增 Offscreen。

本轮已将源码 Editor 的原生库恢复为当前源码的 release 构建，未改动参数事件或求值业务逻辑。旧库备份在 `target/kasane/param-profile/source-editor-debug-backup.dylib`。已经运行的 Godot 进程需要退出重启才能加载替换后的库。

## 同代码 debug / release 对照

环境：Apple M4、Godot 4.7.2 mono、GL Compatibility；窗口 1280×800，实际模型画布 viewport 700×522。独立临时 Editor 加载模型并 fit_view；预热 15 帧，各操作采样 25 次。两种构建使用相同的当前源码（含上一轮 review 修正）、模型和脚本。表中为同步调用 CPU 耗时中位数，不是 GPU 单帧时间。

| 操作 | Mao debug | Mao release | Ren debug | Ren release |
|---|---:|---:|---:|---:|
| get_frame | 48.63 ms | 3.84 ms | 36.63 ms | 2.94 ms |
| set_preview_values，屏蔽信号 | 94.49 ms | 6.70 ms | 70.27 ms | 5.01 ms |
| refresh_geometry | 53.90 ms | 4.22 ms | 40.42 ms | 3.02 ms |
| set_preview_values，含正常信号 | 199.91 ms | 15.74 ms | 149.75 ms | 11.89 ms |
| 实际参数面板滑块回调 | **250.17 ms** | **19.37 ms** | **186.94 ms** | **15.16 ms** |

滑块 CPU 延迟改善分别约 12.9 倍、12.3 倍。包含等待下一 process_frame 的间隔中位数，Mao 从 258.07 ms 降至 25.09 ms，Ren 从 195.83 ms 降至 21.33 ms。因此 release 已显著缓解卡顿，但不能据此宣称连续拖动稳定达到 60 FPS。

## 为什么一次事件会做五次求值

1. `parameter_dock._on_row_value_committed` 调用 `get_frame()`，只是为了读取当前参数，却求值并转换整个模型帧。
2. `DocumentBridge.set_preview_values` 用新值调用 `evaluate_frame()` 做合法性验证。
3. 同步 `preview_changed` 信号触发 `preview.refresh_geometry()`，再次求值并提交预览。
4. 同一信号触发 `parameter_dock.update_values()`，再次调用 `get_frame()`。
5. `set_preview_values` 返回时调用 `get_frame()`，再次求值并转换帧；滑块处理器只使用返回值里的 `ok`。

其中三次 get_frame 还会复制所有 drawable 的顶点、UV、索引及其他帧信息到 Godot 容器。滑块实际需要的是参数值和操作状态。

调用链位置：`apps/editor/ui/docks/parameter_dock.gd`、`apps/editor/main.gd`、`modules/kasane-godot/src/document_bridge.rs` 和 `document_preview.rs`。

## 排除和补充证据

- 从 `ac8339d`（S1 前）提取隔离源码，用同一个原始 Mao.moc3、相同参数序列测量纯 Rust 求值。debug 求值中位数 46.45 ms，当前源码为 48.27 ms，约增加 4%。这不是完整历史 Editor 对照，但说明新增求值逻辑不足以解释本轮十余倍的构建差异。
- Mao 没有 Offscreen，仍表现出 250 ms 级滑块延迟，因此共同主因不是离屏合成。
- 两个模型空闲时的 process_frame 间隔均约 8 ms；参数变化时 CPU 同步调用占主要延迟。
- 测量期间 Mesh 创建数保持不变；Ren 的 Offscreen 创建数保持 24，resize 计数保持 17。没有观察到连续拖动不断创建或调整 Offscreen 的情况。
- `sample` 采样确认 CPU 栈进入 `evaluate_frame`，包含 `binding_for_mesh`、`binding_for_scene`、`blend_bindings_for_target` 和 `resolved_groups`。这些路径重复扫描绑定集合并执行字符串 HashMap 查询，debug 下代价尤其明显。未据此推断每个函数的精确耗时占比。
- 结论针对本次画布尺寸；高分辨率窗口、缩放和更多离屏对象下的 GPU 成本需要另外测量。

## 后续优化顺序

1. 保持正常源码 Editor 使用 release 原生库。Godot 的 debug 启动模式与 Rust dylib 的优化级别是两回事；当前 gdextension 两种模式都指向同一个 dylib。
2. 增加轻量参数查询/更新入口，让滑块无需获取整帧；更新成功后复用本次已求出的帧。保留现有完整 get_frame API，避免破坏脚本兼容性。
3. 建立按 document revision 和 preview 状态失效的帧缓存，供渲染和 UI 共享。必须覆盖撤销、重做、导入、工程切换以及失败更新，不能只按参数字典缓存。
4. 再优化 evaluator 中的反复线性绑定查找，可先每帧建立一次索引，再考虑按结构 revision 持久缓存。
5. 增加真实滑块事件的性能门禁，分别报告 debug/release、CPU 调用与实际帧间隔；现有“资源复用通过”不能证明交互流畅。

## 复现材料

- 脚本：`tools/profile_editor_params.gd`，创建自己的 Editor 实例，先测 Mao，再测 Ren。
- 原始结果：`target/kasane/param-profile/baseline-debug.json`、`current-release.json`。
- 构建和环境：`target/kasane/param-profile/metadata.json`。
- 纯求值对照：同目录 `pre-m3c-core.log`、`current-core.log`。
- CPU 栈：同目录 `sample.txt`。

示例运行（先以 `tools/build_editor.py` 准备独立 staging 目录，并放入需要对照的 native 库）：

```sh
python3 tools/build_editor.py --output-dir target/parameter-bench
cp tools/profile_editor_params.gd target/parameter-bench/profile.gd
/Applications/Godot_mono.app/Contents/MacOS/Godot \
  --path target/parameter-bench --resolution 1280x800 \
  --script res://profile.gd -- \
  /absolute/path/to/repository /absolute/path/to/result.json
```
