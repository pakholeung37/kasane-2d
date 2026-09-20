# MOC3 v5 写出映射（M1）

从 Document 重新构建小端 `csmMocVersion_50`（文件头值 5）。不读取输入 MOC3，不保存 revive 后的指针或运行模型。覆盖普通参数、完整 Keyform、Part、Mesh、嵌套 Rotation/Warp、绘制属性及遮罩。验收入口与证据见 [M1 验收说明](../M1-ACCEPTANCE.md)。

## 代码与边界

- `evaluation.hpp` / `evaluate_frame`：纯内存求值，输出 renderer 与数据检查共用的 DrawableFrame，不调用编码器。
- `moc3.hpp` / `kasane_moc3`：编码字节、model3.json、纹理槽描述；不访问文件或 Core ABI。
- `package.hpp` / `kasane_package`：独立文件适配器，使用 libpng 解码校验素材，调用必需的 ArtifactValidator，再发布完整目录。
- `moc3_sections.inc`：v5 的完整 152 个 section、显式字段宽度与 count 索引，来源为 Purism `moc3.h`。验收脚本自动核对字段表。
- `moc3_tests.cpp`：同一构造/编辑程序分别链接官方 Core、Purism；不复用已有 MOC3 字节。

## 坐标与身份

根对象位置使用原画像素，X 向右、Y 向下，画布原点从左上角计量：

```text
runtime.x = (source.x - origin.x) / pixels_per_unit
runtime.y = (origin.y - source.y) / pixels_per_unit
runtime.uv = (source.u, 1 - source.v)
```

**父级改变坐标域**：Rotation 的子对象位置是局部运行单位；Warp 的子对象位置是无界归一化网格坐标，`[0,1]²` 是内部网格。嵌套变形器的原点/控制点也遵守父级坐标域。设置父级不会隐式保姿势或转换源数组，调用方显式提交所需局部坐标。Rotation 角度以运行坐标的逆时针度数表示，base_angle 与形态 angle 相加；scale 为非负值，反射为独立标志。

只有根位置转换像素单位；局部坐标不重复乘除 ppu。源 UV 左上角为 `(0,0)`；运行 UV 左下角为 `(0,0)`。三角形第二、第三索引交换；canvas flag 写 1，避免 Core 再次翻转。canvas.origin_y 写 `height - source.origin.y`。

对象 UUID、runtime_id、显示名称独立。内部 ID 全局唯一，运行 ID 在对应对象类型内唯一。写出接受 1–63 字节可打印 ASCII；其他编码/长度明确拒绝，不截断。重命名不改变引用或运行 ID。

## 布局与容量

头部 64 字节，随后 160 个小端 uint32 offset；loader scratch 零填充至 count_info 起点 1984。每段按 64 字节对齐，保留 offset 152–159 为零。count_info 为 64 个非负 int32。只有 loader 指针槽允许按 schema 自动补零，其他段长度必须精确匹配。

offset/count 限制在有符号 32 位可表示范围；位置、UV、形态值为 IEEE float32；拓扑索引为 uint16，每 Mesh 至多 65536 顶点。稳定顶点 ID 编译为稠密索引。当前编辑容量：每 Binding 1–3 个普通非循环参数轴、Warp 每轴 1–1024 单元、显式 draw_order 为 `[-32768,32767]`；越界拒绝，不能截断。未指定 Mesh 顺序时使用创建序号。

| 内容 | section | 编码规则 |
|---|---|---|
| count_info / canvas | 0 / 1 | 固定 256 / 24 字节，canvas 尾部零填充 |
| Part | 2–9 | ID、绑定、形态窗口、启用、Part 父级；父级先于子级 |
| 通用变形器 | 10–18 | 父级先于子级；type 0=Warp、1=Rotation；local_idx 指向对应类型表 |
| Warp | 19–24、101、105 | 绑定/形态窗口、控制点数、单元行列数、quad 标志、颜色窗口 |
| Rotation | 25–28、106 | 绑定/形态窗口、base_angle、颜色窗口 |
| Mesh 指针与 ID | 29–33 | 指针槽为 8 字节零值；ID 为 64 字节 |
| Mesh 绑定/层级 | 34–40 | 绑定、完整形态窗口、可见/启用、Part 和变形器稠密索引；无父级 -1 |
| 纹理与标志 | 41–42 | asset_order 对应纹理槽；bit 0 additive、1 multiplicative、2 double-sided、3 inverted mask |
| 几何窗口 | 43–46 | UV offset 以 float 为单位，拓扑 offset/length 以 uint16 为单位 |
| 遮罩窗口与池 | 47–48、80 | 连续的 Mesh 稠密索引，不是 renderer 临时纹理索引 |
| 普通参数 | 49–57、102–104、114–116 | 范围/默认值/decimal_places；repeat=0、type=0，BlendShape 窗口为空 |
| Part Keyform | 58 | draw_order |
| Warp Keyform | 59–60、137–138 | opacity、位置池 offset、multiply/screen offset |
| Rotation Keyform | 61–67、139–140 | opacity、angle、origin、scale、reflect_x/y、颜色 offset |
| Mesh Keyform | 68–70、141–142 | opacity、draw_order、位置和颜色 offset |
| 位置池 | 71 | 每点两个 float，count 是 float 总数 |
| 轴索引/绑定/关键值表 | 72–77 | table 按参数分组，Binding 显式保留轴顺序；静态绑定 0 为零轴 |
| UV / 索引池 | 78–79 | 每顶点两 float，每三角形三 uint16 |
| 绘制组 | 81–85 | 根组 + 每 Part 一组；min/max 覆盖所有 Keyform，total_count 为后代 Mesh 数 |
| 绘制项 | 86–88 | Mesh type=0、self_group=-1；Part type=1、self_group 指向自己的子组 |
| Mesh 颜色窗口 | 107 | 对象在统一颜色池中的完整形态窗口 |
| 颜色池 | 108–113 | RGB float；默认 multiply=(1,1,1)、screen=(0,0,0) |
| 范围外可选段 | 其余 | 对应 count=0，offset 为当前对齐游标；不声明支持 Glue/BlendShape/Offscreen |

Keyform 轴 0 最快变化；每个明确关键值组合必须恰好出现一次。参数键值枚举是所引用轴关键值的去重并集。单关键值轴可产生额外的不可达重复形态以满足 Core gather scratch 窗口，实际 key_len 仍为真实组合数，不补造缺失组合。

Reflection 按 Core 取当前组合中第一个形态的离散值，其他 Rotation 属性线性插值。Warp 三角、quad 内插以及近/远边界外推、嵌套 Rotation 方向计算都调用提取的 Purism 原算法。

## 输出与失败原子性

`publish_package` 先编码全部源形态、读入并完整解码 PNG、检查尺寸，再执行调用方提供的运行时验证。没有验证器则拒绝发布。随后在同级临时目录写 `model.moc3`、`model.model3.json`、`textures/*.png`、`export-report.json`，完成后替换目标；失败恢复旧目录。若系统同时阻止回滚，错误会明确给出仍保存旧产物的备份目录，不报告成功。

源数据编辑不读取图片。PNG 丢失、损坏、尺寸不符、运行时验证拒绝、写入失败均在发布边界报告。PNG 解码上限为 1 GiB RGBA。工程快照和运行包是不同用途：Godot 源快照 format_version=5，完整保存本轮字段；旧版本 1–4 明确拒绝。

运行时禁用对象可能保留历史的顶点、顺序、颜色和透明度缓存；无状态 DrawableFrame 明确报告 enabled/visible，并清空禁用几何。对照不比较禁用对象未定义的历史通道，但仍验证对象身份、拓扑、UV、关系、遮罩、可见性、最终 render_order，以及所有启用对象的全部数值。透明度为零但仍 enabled 的对象保留有效几何供遮罩使用。
