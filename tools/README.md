# 验证工具

## 当前入口

| 入口 | 用途 |
| --- | --- |
| `validate_sdk.py` | 在仓库外临时环境安装 wheel，运行 CPU/GPU、创作、导入编辑、导出、双 Core 与图像参考门禁，并保存报告与哈希。 |
| `sdk_experiments.py` | 冻结 SDK 实验条件，生成三个渐进受试任务，记录外部 agent 进程，独立判分和重放，保存审阅与汇总。见 [实验协议](../docs/SDK-EXPERIMENTS.md)。 |
| `compare_sdk_image.py` | 读取 RGBA8 PNG，比较整图和 focus crop，保存差分图。 |
| `compare_wgpu_blends.py` | 将 720 个 WGPU 混合样本与固定的官方 Framework GPU 参考图比较。 |
| `validate_official_core.py`、`probes/` | 对 Rust MOC3 导出运行官方与 Purism Core 数值对照。 |
| `create_*_fixture.py` | 生成被 Rust/Python 测试复用的 MOC3 fixture。 |
| `run_purism_fuzz.sh`、`check_purism_bundle.sh` | Purism Core 模糊测试与单头文件 smoke test。 |

```sh
uv run --locked python -m unittest discover -s tests -p test_acceptance_validators.py -v
uv build --wheel --python 3.14 --out-dir target/python-wheels modules/kasane-python
uv run --locked python tools/validate_sdk.py --wheel /absolute/path/to/kasane.whl
uv run --locked python tools/compare_wgpu_blends.py
```

带 `observe` feature 的 wheel 可使用 `--require-gpu --require-image-reference`。完整本机门禁再加 `--full --official-probe /absolute/path/to/kasane_document_official_probe --purism-probe /absolute/path/to/kasane_document_purism_probe`。`--full` 要求 GPU、双 Core 和固定参考图实际通过。CPU wheel 的 GPU 项会报告 `not_run`，总状态为 `partial`。

参考图是历史外部运行时捕获，不在每次运行时重新生成。其模型、纹理、PNG 哈希和视图参数在 `tests/fixtures/render_reference/`；任何参考图更新都应留下新的来源与对照证据。

`stage_godot_addon.py` 服务于独立的 Cubism benchmark，用于把 `gd-cubism` 插件暂存到 benchmark 的 Godot 工程。
