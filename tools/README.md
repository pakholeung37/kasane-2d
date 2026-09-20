# Kasane 2D 验证工具与外部真值探针 (Tools & Probes)

本目录包含 Kasane 2D 的自动化验收门禁、视觉回归工具以及用于客观真值对照的**外部运行时探针（Probes）**。

在项目全量迁移至纯 Rust 架构后，所有遗留的 C++ 业务代码已彻底退役；本目录保留并维护用于保证**工业级兼容性**与**绝对正确性**的验证工具链。

---

## 目录索引与工具职责

| 工具 / 脚本 | 职责与验证目标 | 运行前提 |
|---|---|---|
| [`tools/probes/`](probes/) | 独立编译的外部运行时真值探针源码（C++20 极简探针程序） | CMake, 官方 SDK 或 PurismCore |
| [`tools/validate_official_core.py`](validate_official_core.py) | **P0 官方 Core 对照门禁**：验证 Rust 导出的 MOC3 在官方 Core 下的顶点几何、图层排序、透明度及遮罩 | Python 3, 官方 Native SDK |
| [`tools/validate_gpu.py`](validate_gpu.py) | **P0 真实 GPU 视觉回归**：在真实显示渲染管线下，逐像素比对 Rust 预览节点与官方 Core 的画面一致性 | Godot 4.3+, `numpy`, `pillow` |
| [`tools/compare_gpu_images.py`](compare_gpu_images.py) | GPU 画面像素差分计算模块（全画幅均值与 5x5 特征区域灵敏分析） | `numpy`, `pillow` |
| [`tools/validate_godot.py`](validate_godot.py) | **P0 Godot 生命周期与工作流闭环**：验证句柄失效保护、信号重入安全与新建→编辑→保存→导出全流程 | Godot 4.3+, Rust GDExtension |
| [`tools/test_acceptance_validators.py`](test_acceptance_validators.py) | 验收验证器本身的负控制单元测试（防止门禁自身出现假阳性/假阴性） | Python 3 `unittest` |
| [`tools/stage_godot_addon.py`](stage_godot_addon.py) | 将 `gd_cubism` 运行时插件注入到示例/测试 Godot 项目中 | Python 3 |
| [`tools/run_cubism_core_demo.sh`](run_cubism_core_demo.sh) | 驱动运行交互式 Godot 演示项目 | Godot, SCons, CMake |
| [`tools/run_purism_fuzz.sh`](run_purism_fuzz.sh) | PurismCore 解码器 libFuzzer 模糊测试脚本 | Clang / LLVM |

---

## 核心概念：什么是“探针”（Probes）？它是干什么用的？

位于 [`tools/probes/`](probes/) 的探针程序是本代码库最重要的**客观真值参照体系（Ground Truth Oracle）**。

### 1. 为什么需要探针？
- **避免循环自证（Self-referential Fallacy）**：
  若仅使用 Kasane 自身的 Rust 求值器来验证自身导出的 MOC3 文件，无论出现何种规范偏差或数学错误，测试都可能“全绿”。
  Kasane 2D 的核心承诺是：**导出的 MOC3 文件必须能被官方 Live2D 运行时无缝加载并 100% 正确展示**。因此必须使用真实的 Live2D 引擎作为真值标尺。
- **进程与架构解耦**：
  官方 `Live2DCubismCore` 属于专有商业库，且 Kasane 2D 自身是纯 Rust 实现，绝不在正式发布物中捆绑或静态链接任何专有库。
  通过将探针独立为一个单独的可执行小程序，Kasane 可以在测试阶段通过进程间通信调用它，而在生产阶段保持零外部 C 库依赖。

### 2. 两个探针的具体用途

探针源码位于 [`tools/probes/project_runtime_probe.cpp`](probes/project_runtime_probe.cpp)，通过 [`tools/probes/CMakeLists.txt`](probes/CMakeLists.txt) 编译为两个独立二进制：

1. **官方真值探针 (`kasane_document_official_probe`)**：
   - **链接目标**：官方 `Live2DCubismCore.a`（位于 `third_party/CubismSdkForNative-5-r.5/Core`）；
   - **核心职责**：调用官方闭源运行时接口（`csmReviveMocInPlace`、`csmInitializeModelInPlace`、`csmUpdateModel`、`csmReadCanvasInfo` 等）；
   - **工作方式**：接收标准输入传入的多维参数矩阵采样点，驱动官方核心计算出顶点坐标、图层 Draw Order、Render Order、不透明度、正片叠底/滤色通道以及剪贴遮罩索引，输出标准 JSON 数据；
   - **验收指标**：Rust 导出模型的求值结果与官方 Core 的最大欧氏几何误差必须 $\le 0.05$ 像素。
2. **开源对照探针 (`kasane_document_purism_probe`)**：
   - **链接目标**：C99 开源参考实现 [`modules/purism-core`](../modules/purism-core)；
   - **核心职责**：在无法配置官方闭源 SDK 的平台或纯开源环境下，提供标准 C-ABI 的二次验证基准。

### 3. 探针的运行流程图

```
 ┌──────────────────────────────────────────────┐
 │ Kasane Rust 核心 (modules/kasane-moc3)       │
 └──────────────────────┬───────────────────────┘
                        │ 1. 导出模型 (model.moc3)
                        ▼
 ┌──────────────────────────────────────────────┐
 │ 验证调度器 (tools/validate_official_core.py) │
 └───────┬──────────────────────────────┬───────┘
         │ 2. 传入参数采样点 (stdin)    │ 3. 传入同一组采样点
         ▼                              ▼
 ┌───────────────────────────┐  ┌───────────────────────────┐
 │ 外部探针: Official Probe  │  │ Kasane 内置 Rust 求值器   │
 │ (Live2DCubismCore 官方库) │  │ (modules/kasane-core)     │
 └─────────────┬─────────────┘  └─────────────┬─────────────┘
               │ 4. 真实渲染参数 (JSON)       │ 5. Rust 求值结果
               └──────────────┬───────────────┘
                              ▼
               ┌──────────────────────────────┐
               │ 差分断言器                   │
               │ - 几何距离 <= 0.05 px        │
               │ - 渲染层序 100% 一致         │
               │ - 遮罩与颜色通道 100% 一致   │
               └──────────────────────────────┘
```

---

## 常用测试命令指南

### 1. 运行官方 Core 外部真值对照
```sh
# 依赖 third_party/CubismSdkForNative-5-r.5
python3 tools/validate_official_core.py
```

### 2. 运行真实 GPU 视觉回归测试
```sh
# 需要在拥有图形显示环境的机器上执行（依赖 Godot 与真实 GPU）
cargo build --release -p kasane-godot --locked
python3 tools/validate_gpu.py --library target/release/libkasane_godot.dylib
```

### 3. 运行 Godot 生命周期边界与完整创作工作流
```sh
# 测试包括：句柄失效保护、信号重入安全、50 轮工程重载压测、新建→编辑→保存→重开→导出 MOC3
python3 tools/validate_godot.py --library target/release/libkasane_godot.dylib --suite all
```

### 4. 运行验证器负控制测试
```sh
# 验证当顶点异常、数值溢出 (NaN/Inf)、数组截断时，门禁能正确阻断失败
python3 -m unittest discover -s tools -p test_acceptance_validators.py -v
```
