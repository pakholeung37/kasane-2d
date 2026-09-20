# Kasane Editor

Godot Agent-first 模型编辑器。应用在 `apps/editor/`，与开发启动使用同一份代码；不引用 demo 或 fixture。目标平台当前已实际构建和验证 **macOS arm64**。Linux / Windows 配置保留，尚未进行对应平台验收。

## 使用打包应用

解压 `dist/editor/Kasane-Editor.zip`，启动 `Kasane Editor.app`，无需安装 Godot、Rust 或 addon。包内包含 Godot 引擎、`kasane-godot` 原生库、嵌入原生库的 renderer shader 和应用脚本。macOS 包使用 ad-hoc 签名，未做 Developer ID 公证。

工程栏提供新建、打开、保存、另存为、PNG 导入、model3 导入和 MOC3 包导出。PNG 导入使用当前画布作为原画尺寸，支持显式裁剪偏移；初始网格可继续通过脚本编辑。对象树显示组织关系，属性面板单独跳转变形父节点，并选择 Binding / Keyform。参数预览不会修改 Keyform。

Agent 脚本面板显示本地请求目录，可手动执行 `.gd` 文件，也可由外部 Agent 自动提交。接口类型、字典字段、坐标、数组写回和错误约定见 [API.md](API.md)。最小脚本：

```gdscript
extends RefCounted
func run(w) -> Dictionary:
    return w.new_project(Vector2(1024, 1024))
```

自动提交（应用已经启动）：

```sh
python3 tools/editor_agent.py --directory /应用显示的Agent目录 --script /absolute/edit.gd --observe
```

整段脚本在主线程同步执行。请求携带执行 ID、app_id 和文档代次，结果包括业务返回值、日志、错误行号、起止 revision、观察图片与元数据。超时不会取消脚本；重试使用原执行 ID。所有模型操作返回结构化结果，调用方应检查 `ok` 后再执行依赖操作。

## 开发启动

直接在 Godot 中打开源码项目之前，先在仓库根目录准备原生依赖：

```sh
python3 tools/build_editor.py --prepare-source
```

完成后关闭并重新打开 `apps/editor/project.godot`，再按 F6/F5 运行。也可加 `--editor` 直接打开已准备好的项目。命令会编译 Rust 原生库，在源码项目内生成被 Git 忽略的 `native/` 和 `kasane.gdextension`；修改 Rust 后重新运行。仅打开 `.godot` 项目不会自动编译或加载仓库外的原生库。缺少依赖时应用会显示准备命令。

以下命令仍在 `target/editor-app/` 中准备并启动独立开发副本：

```sh
python3 tools/build_editor.py --run
python3 tools/build_editor.py --test
```

默认 Godot 路径为 `/Applications/Godot_mono.app/Contents/MacOS/Godot`，可通过 `--godot` 指定标准版。默认编译 release 原生库，再复制应用与库到 `target/editor-app/`；`--skip-build` 复用已有库。原生库通过临时文件原子替换，避免覆盖运行中的映射文件。构建入口先实际启动应用检查加载/脚本错误，防止 Godot 返回 0 却包含脚本错误时发布坏包。

可用 `Godot --path target/editor-app -- --agent-dir=/absolute/queue` 指定独立请求目录。同一目录只允许一个应用进程。开发启动不需要运行 Godot 编辑器导入资源。

## 构建桌面包

准备同版本的 Godot 标准版与导出模板（本轮为 4.7.2），从官方 [版本下载页](https://godotengine.org/download/archive/4.7.2-stable/) 获取。macOS 使用模板归档中的 `templates/macos.zip`：

```sh
python3 tools/build_editor.py \
  --godot /absolute/Godot.app/Contents/MacOS/Godot \
  --template /absolute/templates/macos.zip \
  --export dist/editor/Kasane-Editor.zip
```

构建入口在干净 staging 目录中打包，避免混入测试产物；官方 universal 模板先通过 `lipo` 提取当前架构，再与同架构原生库一起导出。Godot 使用 **debug export template**，Rust 使用 **release library**。这是脚本编辑器的交付配置：Godot release 模板会省略部分运行时脚本错误检查，不能保证错误行号与失败结果语义。Debug 模板是随包引擎，不要求用户安装开发环境。

本机 Godot 4.7.2 的首次 headless editor import 退出可能崩溃，标准版和 Mono 版都观察到过；正常应用启动、显式 export 及带窗口的播放器资源导入分别验证，不把这个已知问题当作跳过门禁的理由。

## 验收与证据

```sh
python3 tools/validate_m5.py --application dist/editor/Kasane-Editor.zip \
  --godot /absolute/Godot.app/Contents/MacOS/Godot
python3 tools/validate_godot.py --suite all
cargo test -p kasane-core --tests --locked
cargo test -p kasane-godot --lib --locked
```

M5 入口将应用解压到仓库外的独立临时目录，以空 HOME、受限 PATH 启动打包可执行文件；全部输入和工程数据复制/创建在该目录。通过本地请求执行 PNG 建模、外部 M3 模型编辑、持续修改、参数采样、保存重开、导出、错误恢复、UI 与脚本一致性及接口检查。退出编辑器后，再用已有官方/Purism Core 探针和 gd-cubism 播放器核对导出数据与画面。`--probes` 可指定现有运行时探针目录；首次准备探针请参照 `tools/validate_m3.py`。

报告位于 `target/kasane/m5/report.json`，包含 Git/submodule revision、平台、引擎/应用 SHA、输入素材 SHA、逐项状态及外部证据目录。外部模型使用 M3 的 `3d8e869a678a1dac.moc3` 与同一验收纹理映射，输入未重新生成。官方 SDK、外部模型与模板留在本机，不提交进仓库。

独立的 Agent 自检记录由 `--agent-review` 指定，默认 `target/kasane/m5-agent-review/review.json`。它来自实际读取数据和观察图后进行的持续修正，包含对象 ID、脚本、修改前后截图和诊断；验收脚本不会生成或冒充这份人工/Agent 判断记录。缺少此记录或其他必需条件时，M5 报告不通过。
