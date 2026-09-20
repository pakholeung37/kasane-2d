# M5 验收记录

2026-09-20：M5 在 macOS arm64 完成正式打包应用验收。实现范围见 [里程碑](M5-agent-editor.md)，使用与构建见 [Editor README](../../apps/editor/README.md)，脚本契约见 [API](../../apps/editor/API.md)。Linux / Windows 尚未验收。

## 交付

- 应用包：`dist/editor/Kasane-Editor.zip`，解压后直接启动 `Kasane Editor.app`。
- Godot 4.7.2 debug export template + release Rust 原生库；引擎、原生库、renderer 与应用脚本随包加载。保留 debug 模板是为了完整捕获运行时脚本错误，不要求使用者安装 Godot。
- 新建/打开/保存工程、PNG 建模、MOC3 导入和导出、完整对象与 Keyform 脚本编辑、参数预览、公共 renderer、对象检查、显式 UndoRedo Action、本地串行脚本请求与版本化截图均已接通。
- 修复了嵌套数组转顶点时静默归零、删除后同 ID 对象使旧句柄复活、多个变形器 runtime ID 冲突等闭环中发现的问题。

## 可复现入口

```sh
python3 tools/build_editor.py \
  --godot target/godot-tools/standard/Godot.app/Contents/MacOS/Godot \
  --template target/godot-tools/templates/macos.zip \
  --export dist/editor/Kasane-Editor.zip
python3 tools/validate_m5.py --application dist/editor/Kasane-Editor.zip
python3 tools/validate_godot.py --suite all \
  --godot target/godot-tools/standard/Godot.app/Contents/MacOS/Godot \
  --output-dir target/m5-godot-regression
cargo test -p kasane-core --tests --locked
cargo test -p kasane-godot --lib --locked
```

Godot 标准版、导出模板、官方/Purism 运行时探针及外部输入需按应用说明和 M3 验收说明准备；不提交第三方二进制或素材。Agent 自检记录来自独立的实际交互，默认读取 `target/kasane/m5-agent-review/review.json`；缺失时整体验收不通过。

## 结果

| 门禁 | 结果 |
|---|---|
| 独立应用 | 仓库外解压，以独立 HOME、`PATH=/usr/bin:/bin` 启动包内可执行文件，完成编辑、保存、导出 |
| PNG 建模 | 两张不同尺寸和位置的裁剪 PNG；Part、mesh、两种 Rotation/Warp 嵌套顺序、参数和 Keyform；9 个样本 |
| 外部模型编辑 | M3 外部原始 MOC3 输入；修改几何、变形关系、绑定与绘制属性；123 个样本 |
| 持续修改与重开 | 后续脚本使用原对象 ID；保存重开数值误差为 0 |
| 应用接口 | 打包应用内 83 项检查通过，包括 UI/脚本一致性、错误行号、保留已完成写入、显式 Action 恢复、旧句柄失效 |
| 执行协议 | 8 项通过：去重、编译错误、运行错误、旧代次、未完成 claim、不存在对象截图、图片写入失败、队列独占 |
| 官方 Core | PNG 最大位置误差 0.000024 px；外部模型 0.000628 px |
| Purism Core | PNG 最大位置误差 0.000012 px；外部模型 0.000837 px |
| 独立播放器 | 退出编辑器后加载导出包；10 张全图、2 张对象裁剪图的像素差异均为 0 |
| 原生绑定回归 | boundary 59 + lifecycle 55 + workflow 37，共 151 项通过 |
| Rust 回归 | core 的 3 项集成测试、godot 的 6 项单元测试通过 |
| 界面复核 | 1280×800 窗口完整显示工具栏、对象树、画布、属性、参数、日志与底部状态 |

## 证据

主报告：`target/kasane/m5/report.json`，记录应用 SHA256、Git/submodule 状态、引擎、输入 SHA、逐项结果和独立目录路径。接口详细记录、工程、请求/结果、导出包、运行时采样、截图与差异图保存在报告指定的独立目录。最终 UI 截图为该目录的 `application-ui.png`。

本轮应用 SHA256：`ac1454f6b62d1caba0c0df7c586b76c0e5ce9e47c2b79157b0ead49e7f1da2c7`。

Agent 自检证据在 `target/kasane/m5-agent-review/`：读取首次空白观察图和 drawable 数据，定位坐标及嵌套数组转换问题；保留对象 ID 修正后再检查图像，发现第二层方向倒置，修正 Rotation 基准角后再次观察，确认两层绿色角标均位于左上。记录包含三次脚本、结果和前后图，不由验收入口生成。回归报告为 `target/m5-godot-regression/report.json`。

以上是本机实际运行结果；`target/`、`dist/` 和独立临时目录均为本地证据与产物，不纳入源码提交。

源码开发启动补验：`python3 tools/build_editor.py --prepare-source --skip-build --test` 在 `apps/editor/` 生成本地原生依赖后，10 项工作区检查通过。移除原生依赖的独立副本显示明确准备命令，不再出现 Nil 访问异常。直接打开源码项目前必须先运行准备命令，之后关闭并重新打开 Godot 项目。
