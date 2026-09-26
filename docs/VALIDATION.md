# 当前验证入口

面向陌生 agent 的渐进使用实验见 [SDK 实验底座](SDK-EXPERIMENTS.md)。
它使用独立任务与受试轨迹衡量可用性，不替代下面的 SDK 正确性回归。

## 自动化回归

```sh
cargo fmt --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
KASANE_MOC3_DISABLE_CORE_VALIDATION=1 cargo test -p kasane-moc3 --no-default-features --test safety_tests --locked
uv run --locked python -m unittest discover -s tests -p test_acceptance_validators.py -v
```

Rust SDK 测试覆盖隔离编辑、句柄、history、导入、工程保存和导出；WGPU 测试检查实际像素、遮罩、混合、离屏组合与 resize。`tests/test_acceptance_validators.py` 检查验收门禁能拒绝损坏的输入和参考图。`tests/fixtures/` 是这些测试及 Python wheel 共同使用的输入，不依赖 Godot 工程。

## 仓库外 wheel 集成测试

```sh
uv build --wheel --python 3.14 --out-dir target/python-wheels modules/kasane-python
uv run --locked python tools/validate_sdk.py --wheel /absolute/path/to/kasane.whl
```

门禁在临时虚拟环境安装 wheel，从仓库外运行 Python CPU 测试、两素材创作/保存/导出与外部 model3 导入编辑；保留输入哈希、命令日志与结果。CPU wheel 无 GPU 功能时报告 `partial`，不是完整验收。

在有 GPU 和本地 Core 探针的环境，使用带 `observe` feature 的 wheel：

```sh
uv run --locked python tools/validate_sdk.py --full \
  --wheel /absolute/path/to/observe-wheel.whl \
  --official-probe /absolute/path/to/kasane_document_official_probe \
  --purism-probe /absolute/path/to/kasane_document_purism_probe
uv run --locked python tools/compare_wgpu_blends.py
```

`--full` 要求 10 项检查均实际通过：CPU、两条创作/导入流程、GPU、第二脚本修正、两套 Core 对新建与导入模型的数值对照、固定外部 GPU 图像参考。输入或参考图哈希不一致、焦点裁剪误差超限、任一必需项缺席均失败。参考图来源与生成时的输入、视图和纹理配置见 `tests/fixtures/render_reference/`；更新参考图须重新记录来源并运行负控制。

Observe 的独立 wheel 发布门禁可直接运行：

```sh
uvx maturin build -m modules/kasane-python/Cargo.toml --features observe --release --locked -o /absolute/path/to/wheels
python tools/validate_sdk.py --wheel /absolute/path/to/kasane.whl \
  --python /absolute/path/to/python3.14 --require-inspection \
  --output /absolute/path/to/evidence
```

此 profile 在仓库外临时环境安装 wheel 的 `inspection` extra，强制运行基础 GPU、O3–O7 inspection 和 CPU 测试，并保留输入哈希及日志；缺 GPU 功能、任一测试失败均失败。`--inspection` 使用相同套件，但允许 CPU wheel 以 `partial` 结束。与旧 `--full` 的两套 Core 和外部图像门禁互不替代；`--full --inspection` 可在具备旧脚本及探针的环境同时运行两组检查。

O3–O7 覆盖配准、二维布局、形变、mask/隔离、几何与 GPU coverage 查询、动画 recipe 回放及失败报告。CPU-only wheel 无需 `inspection` extra 即可打开已保存的 v2 报告；新建离线 PNG 对比则需要该 extra。性能复测使用已安装的 GPU wheel：

```sh
python tools/run_observe_visual_o7_benchmark.py --output /absolute/path/to/benchmark.json
python tools/run_observe_visual_baseline.py --output /absolute/path/to/raw-baseline.json
```

CI 运行 Rust 回归、验证器负控制和 CPU wheel 集成测试；macOS runner 另外运行 WGPU 混合矩阵。真实 GPU 观察与双 Core 的完整门禁按有相应资源的环境运行，报告不把 `not_run` 计作通过。

历史 Godot 里程碑与门槛记录仍可从 Git 历史及 `docs/archive/` 查阅。
