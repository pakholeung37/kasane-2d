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

`kasane_render::ScenePlan` is the persistent logical boundary used by Godot.
It owns explicit target-local draw/composite lists and raw mask-source
references, without retaining a published frame or positions. The Godot
adapter lowers it to physical mask/viewports and attachment layouts. Camera
updates use a separate `update_view` path, skipping frame validation, geometry
synchronization and color-node reordering. Texture binding changes preserve
mesh geometry, and mask resolution changes reuse existing viewports within a
sampling policy.

`prepare_frame` / `PreparedFrame` remain a compatibility facade generated from
that same logical scene. Their pass stream preserves legacy scene-assembly
order; it is not a GPU execution schedule. The shared layer does not require a
backend to reproduce Godot's consumer-specific mask instances.

`kasane-render-wgpu` keeps device and queue ownership with its host and
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

## Recorded comparisons and view-only contracts

- [Extraction baseline](results/20260923-backend-extraction/summary.md): c21c111 vs de62a6b.
- [Persistent scene plan](results/20260923-scene-plan/summary.md): de62a6b vs the first scene/view refactor.

`refresh_cpu_ms` includes the full Dictionary returned by `set_preview_values`;
keep this endpoint unchanged for historical comparisons. It is not isolated
renderer CPU time. `frame_ms` also includes host scheduling waits.

Run the targeted camera/mask/texture and lifecycle contracts with the freshly
built Release library:

```sh
python3 tools/validate_render_boundary.py
```

`scene_submissions`, `view_updates`, `geometry_syncs` and `mask_creations` in
render stats distinguish camera layout work from model-frame synchronization.
The parameter benchmarks above do not quantify camera-only CPU latency.


The [incremental model-sync comparison](results/20260923-incremental-sync/summary.md)
compares against the first ScenePlan implementation. `order_syncs` and
`material_syncs` expose skipped native node and drawable material updates.
