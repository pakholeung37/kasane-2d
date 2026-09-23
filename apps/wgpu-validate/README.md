# Real-model WGPU validation host

Run the Godot/WGPU comparison from the repository root:

```sh
python3 tools/validate_wgpu_real_models.py
```

The suite runs both the default pose and `ParamAngleX=15`, checks that both
renderers change images, and writes an aggregate report under
`target/wgpu-real-model-suite/`. To capture one selected pose instead, run
`python3 tools/compare_wgpu_real_model.py` with the options below.

The single-case command defaults to the SDK's Ren model and renders its default
pose at 2048×2048 with Godot's current linear mipmap texture path. It builds
the current Godot extension, writes one shared `case.json`, captures both
renderers, and writes frame summaries, PNGs, a raw difference image and
`report.json` under `target/wgpu-real-model/`. The image
report checks the full image, foreground bands, and each offscreen object's
evaluated bounds when nonempty. The script requires NumPy and Pillow; it uses the
Codex bundled Python runtime when the system Python lacks them. Failure
returns a nonzero status. Use `--model3`, `--godot`, or `--output-dir` to select
other inputs. Parameter values use model runtime IDs. For example:

```sh
python3 tools/compare_wgpu_real_model.py \
  --parameter ParamAngleX=15 \
  --output-dir target/wgpu-real-model-angle-x-15
```

Use `--texture-profile linear_no_mipmap` to isolate sampling from composition.
Both paths report the SHA-256 of their full mip chains, so a matching image
cannot hide different texture preprocessing. The host executable can also run
by itself:

```sh
cargo run -p kasane-wgpu-validate --locked -- target/wgpu-real-model/case.json target/wgpu-real-model
```

The same model and texture loader can display directly to a WGPU window:

```sh
cargo run -p kasane-wgpu-viewer --locked -- \
  target/wgpu-real-model/case.json
```

The viewer resizes its output to the window and presents without GPU-to-CPU
readback. It is a rendering shell, not the new editor UI. The existing Godot
application remains available throughout migration. A one-shot window and
resize smoke run is available with `--frames 2 --smoke-resize` after the case
path.
