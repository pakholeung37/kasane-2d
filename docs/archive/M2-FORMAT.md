# Kasane 目录工程格式

格式标识为 `kasane-directory-project`，版本为 `1`。它与旧实验 `kasane-project` 的 v1–v5 没有版本继承关系。旧标识一律返回 `LEGACY_PROJECT`，包含版本与拒绝原因；新标识的未知版本返回 `UNSUPPORTED_VERSION`。

## 目录与 API

```text
my-model/
  project.kasane.json
  assets/
    <sha256>.png
```

`KasaneProjectIO.save_project(document, path)` / `open_project(document, path)` 接受工程目录，或以 `.json` 结尾的清单路径；目录参数使用 `project.kasane.json`。清单所在目录是工程根。另存为就是向另一个目录调用 `save_project`。保存和打开无需场景树、画布或 GPU，也不调用 MOC3 编码器。

生产 API 只接受本机绝对路径，不解析 `res://`、`user://` 或相对工作目录。UI 在选择文件时提供系统路径。

`modules/kasane-document` 是独立原生模块：`DocumentSession` 持有 Document、工程路径和保存基线；`DocumentStore` 实现保存与打开；codec 使用 nlohmann JSON，PNG 使用 libpng，SHA-256 使用 OpenSSL。内部 `FileSystem` 接口统一读写、独占创建、锁、重命名与同步。标准库提供普通路径和读取操作，平台 API 提供标准库不能表达的操作。测试注入同一个接口的失败，不保留 Godot 文件系统后端。

Godot 适配层只转换脚本参数、结果、信号和 RGBA 纹理；持久化测试可以完全不启动 Godot。

## 字段覆盖表

根对象必需 `format: string`、`format_version: uint32`、`document: object`。下表覆盖全部正式 M1 源字段。对象列表始终按 Document 的源顺序保存；Part/Transform 解码先建立对象再通过正式替换 API 安装父级，允许父对象位于列表后面。所有引用都是稳定 ID，禁止悬空引用、重复 ID、环、非法拓扑和不完整 Keyform。

| 源类型 | 持久化字段 | JSON 类型 / 约束 |
|---|---|---|
| Document / Canvas | `id`, `canvas`, `canvas_origin`, `pixels_per_unit` | UUID 字符串；两个有限正尺寸、两个有限原点数、有限正 PPU |
| ImageAsset | `id`, `name`, `source`, `width`, `height`, `sha256` | UUID/字符串；`assets/` 下规范相对路径；正 uint32 尺寸；64 位十六进制 SHA-256 |
| Part | `id`, `runtime_id`, `name`, `parent_id`, `enabled`, `draw_order` | 字符串、bool、有限数；空父 ID 代表根 |
| Transform | `id`, `runtime_id`, `name`, `part_id`, `parent_id`, `kind`, `base_angle`, `rotation`, `rows`, `columns`, `quad`, `enabled`, `points`, `appearance` | kind 为 0 Warp / 1 Rotation；网格整数/控制点数量经 Document 校验；根使用空父 ID |
| RotationPose | `origin`, `angle`, `scale`, `reflect_x`, `reflect_y` | 二元坐标、有限角度/缩放、bool |
| ArtMesh | `id`, `runtime_id`, `name`, `texture_asset_id`, `vertex_ids`, `base_positions`, `uvs`, `triangles`, `properties` | 稳定 uint32 顶点 ID；二维坐标数组；三角形为按三项分组的**顶点 ID**，不是数组下标 |
| Mesh properties | `part_id`, `deformer_id`, `appearance`, `draw_order`, `blend_mode`, `enabled`, `double_sided`, `inverted_mask`, `masks` | blend 0 normal / 1 additive / 2 multiplicative；masks 为 Mesh ID 数组 |
| Appearance | `opacity`, `multiply`, `screen` | 有限透明度及两个 RGB 三元数组；范围经 Document 校验 |
| Parameter | `id`, `runtime_id`, `name`, `minimum`, `maximum`, `default_value`, `decimal_places` | 有限数值范围；整数精度 0–9 |
| MeshBinding | `id`, `mesh_id`, `axes`, `keyforms` | 1–3 个显式有序轴；完整组合 |
| SceneBinding | `id`, `target_id`, `axes`, `keyforms` | Part 或 Transform ID；完整组合 |
| BindingAxis | `parameter_id`, `keys` | 参数 ID、严格递增有限关键值数组；轴顺序保留 |
| MeshKeyform | `keys`, `positions`, `appearance`, `draw_order` | keys 按轴顺序；完整位置数组；组合以轴 0 最快的顺序规范化 |
| SceneKeyform | `keys`, `positions`, `rotation`, `appearance`, `draw_order` | 正式对象的完整关键形态 |

Document 中 `assets`、`parts`、`transforms`、`meshes`、`parameters`、`bindings`、`scene_bindings` 均为必需数组，允许为空。表中字段均由写出器显式保存，但未指定的 Mesh / MeshKeyform `draw_order` 不写出。读取时此字段缺省表示未指定；Parameter 的 `decimal_places` 缺省为 6；Keyform 的 `appearance` 缺省为 opacity=1、multiply=[1,1,1]、screen=[0,0,0]。SceneKeyform 的 `draw_order` 缺省为 0。其余表中字段必须存在。旧 deformer/关系数组不是新格式字段；仅接受兼容测试输入中的空数组，非空明确拒绝，不支持旧简单 Warp。

数值源类型为 float32。写出使用原生 JSON 数值序列化，读取检查类型后交由 Document 验证；保证源 float32 往返，不截断坐标。整数必须是可表示范围内的整数值。名称采用 UTF-8；运行 ID 的可表示性由 MOC3 导出器另行校验。

不保存 GPU RID、句柄 generation、内存地址、稠密索引、revision、求值缓存、预览值、选择、相机、脚本实例或撤销栈。文件中的资源路径始终相对工程根；当前打开目录只保留于 DocumentSession。

## 发布与失败语义

1. 获取工程根的独占写锁，并比较打开时记录的清单 SHA-256。另一个编辑器已保存时返回 `PROJECT_CONFLICT`；锁忙返回 `PROJECT_BUSY`。向未打开的既有清单另存为返回 `DESTINATION_EXISTS`，不静默覆盖。
2. 校验源 PNG 的尺寸和哈希。新素材以独占文件创建写入，检查 write、文件同步及 close；已有同名且内容正确的素材复用，损坏占位使用新名字，绝不覆盖旧素材。同步素材目录。
3. 在工程根的独占临时目录写入完整清单，再检查保存基线，原子替换清单。提交前失败保留旧清单、旧素材及 live Document/保存基线；可能留下未引用的新素材，不自动回收。
4. 替换清单后同步工程根。若此时同步失败，结果仍是成功且 `published=true`，同时 `durable=false` 并提供 warnings，准确表示已提交但持久化未确认。更新 Document 的内部相对路径、哈希和保存基线。
5. 打开先构造并验证候选 Document，成功后才替换当前状态。Godot 绑定使旧句柄失效并清除临时预览值；失败保留原状态。

本地文件系统后端使用 POSIX fsync/flock 或 Windows 文件句柄锁、FlushFileBuffers 和 MoveFileEx。macOS/Linux 支持目录同步；Windows 使用 write-through 移动，不能提供相同的目录 fsync 保证。已在 macOS 验证，其他平台尚未实机验收。锁协调遵守同一协议的编辑器，不阻止外部程序直接改写文件，也不合并冲突。运行包目录替换采用备份与回滚，不是整个目录的单次原子交换；回滚失败会返回保留备份的路径。网络盘和断电恢复不在本轮完整事务保证内。

## 资源诊断与替换

`open_project` / `diagnose_resources` 返回 `ok`、`resources_complete` 和 `diagnostics`。结构合法但 PNG 缺失、无法解码、尺寸或哈希不符时，打开源结构成功；诊断包含 `asset_id`、`code`、`message`。预览验证当前字节，失败清除画面，纹理加载失败清除对应缓存，禁止使用缓存旧图伪装完整。资源异常阻止保存和运行包导出。

- `relocate_asset(document, asset_id, path)`：要求新路径 PNG 尺寸及已有内容哈希一致。
- `replace_asset(document, asset_id, path)`：显式接受新内容，更新尺寸和哈希，产生源修改；下次保存复制到工程内部。
- `export_package(document, path)`：验证并快照素材字节后，复用 M1 原生 `publish_package` 和 `encode_moc3`。失败保留旧运行包；禁止将导出目录设为当前工程或包含源素材的目录。模型兼容性由 M1 和 M2 的双 Core 回归门禁验证，应用导出操作本身不加载两个 Core。

## 保存状态与快照

`modified` 比较全部源字段、列表顺序与最后一次成功保存的内容快照。相同源内容恢复后为 false，即使 revision 已增加；预览和相机不影响它。恢复旧的内存快照不改变当前保存基线。快照用于源数据撤销，不承诺撤销磁盘复制、另存为或素材文件修改。

扩展任何源类型时必须同步更新此表、`project_codec.cpp`、内容相等比较及 原生 `project_tests.cpp` 的往返用例。
