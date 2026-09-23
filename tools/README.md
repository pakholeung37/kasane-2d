# Kasane 2D 验证工具与外部真值探针 (Tools & Probes)

本目录包含 Kasane 2D 的自动化验收门禁、视觉回归工具以及用于客观真值对照的**外部运行时探针（Probes）**。
---

## 目录索引与工具职责

| 工具 / 脚本 | 职责与验证目标 | 运行前提 |
|---|---|---|
| [`tools/probes/`](probes/) | 独立编译的外部运行时真值探针源码（C++20 极简探针程序） | CMake, 官方 SDK 或 PurismCore |
| [`tools/validate_official_core.py`](validate_official_core.py) | **P0 官方 Core 对照门禁**：验证 Rust 导出的 MOC3 在官方 Core 下的顶点几何、图层排序、透明度及遮罩 | Python 3, 官方 Native SDK |
| [`tools/validate_gpu.py`](validate_gpu.py) | **P0 真实 GPU 视觉回归**：在真实显示渲染管线下，逐像素比对 Rust 预览节点与官方 Core 的画面一致性 | Godot 4.3+, `numpy`, `pillow` |
| [`tools/compare_gpu_images.py`](compare_gpu_images.py) | GPU 画面像素差分计算模块（全画幅均值与 5x5 特征区域灵敏分析） | `numpy`, `pillow` |
| [`tools/validate_godot.py`](validate_godot.py) | **P0 Godot 生命周期与工作流闭环**：验证句柄失效保护、信号重入安全与新建→编辑→保存→导出全流程 | Godot 4.3+, Rust GDExtension |
| [`tools/validate_sdk.py`](validate_sdk.py) | SDK S3/S4 wheel 门禁和 S5 两素材新建流程：仓库外临时 venv、CPU/GPU 测试、recipes 和带 hash 的证据报告 | CPython 3.14、已构建的 wheel；GPU wheel 需真实 GPU |
| [`tools/test_acceptance_validators.py`](test_acceptance_validators.py) | 验收验证器本身的负控制单元测试（防止门禁自身出现假阳性/假阴性） | Python 3 `unittest` |
| [`tools/stage_godot_addon.py`](stage_godot_addon.py) | 将 `gd_cubism` 运行时插件注入到示例/测试 Godot 项目中 | Python 3 |
| [`tools/run_cubism_core_demo.sh`](run_cubism_core_demo.sh) | 驱动运行交互式 Godot 演示项目 | Godot, SCons, CMake |
| [`tools/run_purism_fuzz.sh`](run_purism_fuzz.sh) | PurismCore 解码器 libFuzzer 模糊测试脚本 | Clang / LLVM |

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

### 5. 运行 SDK wheel 门禁

```sh
python3.14 tools/validate_sdk.py --wheel /absolute/path/to/kasane.whl --require-gpu
```

默认结果写入 `target/sdk-acceptance/<run-id>/report.json`。CPU wheel 可不加 `--require-gpu` 运行，报告 GPU 项为 `not_run`，总体状态为 `partial`。
