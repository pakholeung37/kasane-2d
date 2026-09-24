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

CI 运行 Rust 回归、验证器负控制和 CPU wheel 集成测试；macOS runner 另外运行 WGPU 混合矩阵。真实 GPU 观察与双 Core 的完整门禁按有相应资源的环境运行，报告不把 `not_run` 计作通过。

历史 Godot 里程碑与门槛记录仍可从 Git 历史及 `docs/archive/` 查阅。
