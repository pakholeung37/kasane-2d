# M2 可搬移构造样例

包含两张程序生成 PNG、多参数 Keyform、嵌套 Warp/Rotation、Part、颜色、混合和遮罩。可复制整个目录到任意本地路径，无需 `.import` 文件。

在已加载 gd-kasane 的 Godot 应用或 headless 脚本中：

```gdscript
var document = KasaneDocumentBridge.new()
var io = KasaneProjectIO.new()
var opened = io.open_project(document, "/absolute/path/m2-complete")
assert(opened.ok and opened.resources_complete)
assert(io.save_project(document, "/absolute/path/my-copy").ok)
assert(io.export_package(document, "/absolute/path/runtime-package").ok)
```

预览连接 `KasaneDocumentPreview` 和 `KasaneTextureStore`；读取工程 PNG 使用直接解码，不依赖 Godot 资源导入缓存。修改源结构使用 Document API。

[格式说明](../../docs/milestones/M2-FORMAT.md) · [验收入口](../../docs/milestones/M2-ACCEPTANCE.md)
