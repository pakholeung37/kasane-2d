# Python SDK 类型速查

`kasane` wheel 内含 `py.typed` 和公开 API 的类型标注。对创作脚本运行 Pyright，可以在执行前发现把属性当方法、拼错字段、误用数据类工具以及未处理可选查询结果等问题。本地已验证的命令是：

```bash
uvx --from pyright==1.1.414 pyright --pythonpath /absolute/venv/bin/python /absolute/script.py
```

`--pythonpath` 应指向**安装了本次测试 wheel 的 Python**。仅在 IDE 中打开仓库源码不等于检查已安装 wheel 的公开接口。

## 常用类型形状

```python
from pathlib import Path
import kasane

session = kasane.open_project(Path("/absolute/project.kasane.json"))
canvas = session.canvas             # 属性，不加 ()
project_path = session.project_path # Path | None，也是属性

mesh = session.require_unique_mesh("right")
geometry = session.geometry(mesh.id)  # GeometrySnapshot | None
assert geometry is not None
assert geometry.space == "parent_local"

binding = session.binding_for_mesh(mesh.id)  # MeshBindingSnapshot | None
assert binding is not None
old = next(form for form in binding.keyforms if form.keys == [1.0])

# Point 是 tuple[float, float] 类型别名；直接用 (x, y)。
# MeshKeyform 是 NamedTuple；_replace 保留 appearance 和 draw_order。
shifted = old._replace(positions=[(x + 0.1, y) for x, y in old.positions])
with session.edit("shift end pose") as edit:
    edit.set_mesh_keyform(binding.id, shifted)

receipt = session.save(Path("/absolute/new-project/project.kasane.json"))
manifest = receipt.manifest  # Path；没有 manifest_path 字段
```

`Session.open_project` 不是入口；打开工程用模块函数 `kasane.open_project`。`MeshKeyform` 和大多数快照是 `NamedTuple`，可用字段访问或 `_replace`，不能用 `dataclasses.replace`。`geometry()`、`binding_for_mesh()` 等按 ID 查询可能返回 `None`，应先检查。`Point` 是类型别名，不是双参数坐标构造器；API 接受普通二元 tuple。

Pyright 只能检查类型契约，不能证明几何正确、资源完整或工程可保存。`DESTINATION_EXISTS` 是保护已有工程的保存冲突，属于运行时状态；新实验应分别检查静态类型和实际保存、重开结果。保存到已有目标时，应先打开该工程再修改，或为新结果选择独立输出路径；不要无条件覆盖用户工程。
