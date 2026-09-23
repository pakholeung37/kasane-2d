# Kasane preview benchmark

This benchmark measures the existing Godot preview path with either a fixed
Kasane fixture or the Ren model's real offscreen workload. It records both the
synchronous CPU refresh cost and the wall-clock time through one rendered
frame, plus stable resource counters.

Run the current checkout in Release mode:

```sh
cargo build --release -p kasane-godot --locked
python3 tools/benchmark_kasane.py \
  --label current \
  --repeats 3 \
  --warmup-frames 60 \
  --sample-frames 600
```

To measure the pre-refactor `HEAD` without changing the working tree:

```sh
git worktree add --detach target/kasane-preview-baseline-src HEAD
env CARGO_TARGET_DIR="$PWD/target/kasane-preview-baseline-cargo" \
  cargo build --release -p kasane-godot --locked \
  --manifest-path target/kasane-preview-baseline-src/Cargo.toml
python3 tools/benchmark_kasane.py \
  --library target/kasane-preview-baseline-cargo/release/libkasane_godot.dylib \
  --output-dir target/kasane-preview-benchmark-baseline \
  --label baseline \
  --repeats 3 \
  --warmup-frames 60 \
  --sample-frames 600
```

Compare the two reports:

```sh
python3 tools/compare_kasane_benchmark.py \
  target/kasane-preview-benchmark-baseline/baseline.json \
  target/kasane-preview-benchmark/current.json
```

The default 5% allowance covers scheduling and GPU-driver noise on a desktop;
mean and p95 are the default gates, while p50 and p99 remain diagnostic. Use
`--include-p99` when running on a controlled machine. A refactor must keep
frame time, refresh CPU time, resource counts, and measured memory within the
configured limit before more renderer extraction is accepted.

## Render boundary

`kasane_render::prepare_frame` now returns a `PreparedFrame` containing both
logical attachment requirements and a backend-neutral pass stream. The stream
contains `Main`, `Offscreen`, `Mask`, `Composite`, `Draw`, and
`EndOffscreen` operations in execution order. `KasaneDocumentPreview` is now a
thin Godot-facing lifecycle/API wrapper; `kasane-render-godot` owns Godot
nodes, viewports, materials, masks, destination copies, shaders, and pass
execution. The backend resolves the prepared operations to Godot resources;
the preview no longer traverses `kasane_core::RenderCommand` or owns renderer
state. `kasane-render-wgpu` keeps device and queue ownership with its host and
accepts host-owned texture views plus reusable backend-owned surface, mask, and
destination pools. `render_scene` currently executes normal, additive, and
multiplicative draws, alpha masks, nested offscreen surfaces, and
destination-reading blend modes. Its main target must include `COPY_SRC` usage
when a visible destination-reading item is submitted to it; backend-owned
offscreen targets include that usage. The `render` and `prepare_basic` entry
points stay limited to flat, unmasked normal draws.

## Real offscreen workload

The flat fixture does not exercise offscreen allocation. Use the existing Ren
model to cover the 24 offscreen surfaces used by the current Godot path:

```sh
python3 tools/benchmark_kasane.py \
  --workload ren-offscreen \
  --label current \
  --output-dir target/kasane-preview-ren-current \
  --repeats 3 \
  --warmup-frames 30 \
  --sample-frames 120
```

Run the same command with the pre-refactor library and a different output
directory, then compare the two reports with the same comparison tool. The
workload imports `Ren.model3.json`, uploads its texture, renders at 2048², and
updates a parameter on every sampled frame so offscreen creation and resize
regressions remain visible in `render_stats`.
