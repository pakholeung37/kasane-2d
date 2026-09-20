# M2 验收与复现

2026-09-20，原生 filesystem 重构后本机功能验收通过。环境为 macOS arm64、Godot 4.7.2、OpenGL Compatibility。持久化本身不依赖 Godot，真实 GPU 仅用于适配层验收。

## 复现

依赖 CMake、C/C++ 编译器、libpng、OpenSSL Crypto、pkg-config。完整双 Core 验收需要 M1 本地官方 Core SDK；Godot/GPU 部分另外需要 Godot 与参考播放器。Python 依赖见 tools/requirements-validation.txt。

```sh
target/kasane/buildenv/bin/python tools/validate_core.py
# 原生持久化验收：不启动 Godot
target/kasane/buildenv/bin/python tools/validate_native_project.py
target/kasane/buildenv/bin/python -m SCons -C modules/kasane-gd platform=macos arch=arm64 target=template_debug -j8
# 原生验收 + 薄绑定 + 真实 GPU
target/kasane/buildenv/bin/python tools/validate_project.py
target/kasane/buildenv/bin/python tools/validate_godot.py
target/kasane/buildenv/bin/python tools/validate_gpu.py
```

不具备官方 SDK 时，可通过根 CMake 的 core-make/core-debug 运行原生单元测试。完整验收必需项缺失时返回失败，不将跳过计为通过。

## 结果

最终 M2 报告：`target/kasane/project-files/run-llr9gn67/report.json`。
其 `native/report.json` 包含 1,034 项原生检查（其中 1,023 项为原生单元检查），另有 37,800 项双 Core 数值比较；外层有 19 项 Godot 绑定/纹理检查和 6 项 GPU 整图/前景比较。

| 范围 | 验证 |
|---|---|
| 源数据 | 全字段 JSON 往返、列表顺序、稳定 ID、完整 Keyform、75 个参数组合求值、MOC3 字节一致 |
| 工程生命周期 | 独立进程保存、搬移、重开、继续编辑、再次保存；model3/MOC3 与采样对照 |
| 写入冲突 | 实际跨进程独占锁；清单 SHA-256 检测过期编辑器；冲突后另存为恢复 |
| 失败注入 | 读取、独占创建、部分写入/磁盘满、文件同步、关闭、目录同步、清单替换失败；旧工程可重开，保存基线保留 |
| 提交状态 | 最终目录同步失败返回已发布与持久化警告，Document 与已提交清单一致 |
| 包发布 | 发布失败恢复旧目录；回滚失败保留备份字节并返回可恢复路径 |
| 素材 | 缺图、损坏、尺寸和哈希不符；重定位拒绝内容变化，显式替换与重开成功 |
| 输入边界 | 非本机/相对路径、符号链接清单、越界素材路径、错误 JSON 类型/版本、循环与不完整 Keyform、重复 JSON 键 |
| Godot | 句柄 generation、信号/状态、错误结果转换、RGBA 纹理和缓存失效；三个参数/相机状态搬移前后 GPU 对照 |

原生 CMake 与 ASan/UBSan 各 8 项 CTest 通过；含官方 Core 的 validate_core 共 9 项 CTest 通过。既有 Godot 59 项边界检查通过，GPU 回归 84 项通过。

报告保留运行环境、源码指纹、逐项结果和证据文件哈希。样例见 [m2-complete](../../samples/m2-complete/README.md)，格式及持久化保证见 [M2-FORMAT](M2-FORMAT.md)。macOS 已实机验收；Linux/Windows 实现尚未在本轮实机验证，不承诺网络盘或完整断电恢复。

## 代码质量

30 个 Kasane/Godot 翻译单元通过 clang-tidy。本次 C++ 文件通过 LLVM 22.1.8 格式检查。全仓格式门禁仍报告 Purism 子模块三个既有文件：include/PurismCore.h、include/PurismKeyform.h、src/param.c；子模块保持干净，不将全仓格式门禁记为通过。
