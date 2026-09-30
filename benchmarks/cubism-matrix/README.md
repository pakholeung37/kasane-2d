# Cubism compatibility and performance matrix

This project uses 40 independent instances of Nijiiro Mao to compare two
Cubism Core ABI providers across Cubism Framework Native and a Core-only runner,
plus an imported Kasane model rendered through native Metal. Compatibility must
be established before performance numbers are treated as valid.

| Case | Core provider | Host/rendering stack | Purpose |
| --- | --- | --- | --- |
| `cubism-native` | Official Cubism Core | Cubism Framework Native | Baseline |
| `purism-native` | Purism Core v6 ABI | Cubism Framework Native | Isolate Purism Core |
| `cubism-core` | Official Cubism Core | Core-only | Pure computation baseline |
| `purism-core` | Purism Core v6 ABI | Core-only | Pure computation comparison |
| `kasane-metal` | Kasane authoring evaluation | kasane-render-metal | End-to-end Kasane native rendering load |

The Core implementation is selected at link time. The matrix therefore creates
separate artifacts; it never switches Core implementations inside a
running process. All Native cases share one C++ runner.

## Layout

- `config/matrix.json` defines the four Core/host combinations and Kasane case.
- `config/mao-40.json` is the shared workload definition. Its 10x4 grid keeps
  each model near the previous on-screen size while doubling update/render load.
- `runners/native/` is the Cubism Framework OpenGL runner.
- `runners/core/` calls only the shared Cubism Core C ABI. It separately measures
  startup, parameter writes, idle/animated updates, drawable readback, and a
  40-model update-plus-readback working set.
- `runners/kasane/` imports Mao once, advances 40 independent animation previews,
  evaluates their geometry, assembles one scene, and submits it with native Metal.
- `tools/matrix.py` validates, prepares, builds, and runs individual cases.
- `results/historical/` preserves measurements from before this restructure.
- `artifacts/results/` and `assets/` are generated/local and ignored. The Mao source is kept under `models/local/mao/` and copied into `assets/` when preparing a rendering case.
- `target/cubism-matrix/build/` holds isolated build artifacts.

## Prerequisites

The proprietary SDK and Mao model are not tracked. Put them at:

```text
third_party/CubismSdkForNative-5-r.5/
models/local/mao/
```

The [model inventory](../../models/README.md) lists their uses and the optional SDK samples.

PurismCore is pinned at `modules/purism-core` as a Git submodule. Initialize
submodules after cloning, or set `PURISM_CORE_ROOT` to use another checkout.
The matrix configures its Purism build with CTest enabled; run it with:

```sh
ctest --test-dir target/cubism-matrix/core/purism-v6 --output-on-failure
```

Validate the matrix alone, or include local prerequisites:

```sh
uv run --locked python benchmarks/cubism-matrix/tools/matrix.py validate
uv run --locked python benchmarks/cubism-matrix/tools/matrix.py validate --local
```

For Native OpenGL builds, prepare the SDK's vendored GLEW and GLFW once:

```sh
cd third_party/CubismSdkForNative-5-r.5/Samples/OpenGL/thirdParty/scripts
./setup_glew_glfw
```

## Native cases

The tool maps `config/mao-40.json` into CMake definitions so the C++ runner
uses the instance count, grid, viewport, warmup, and sampling interval from
the workload definition.

```sh
uv run --locked python benchmarks/cubism-matrix/tools/matrix.py build-native cubism-native
uv run --locked python benchmarks/cubism-matrix/tools/matrix.py run cubism-native

uv run --locked python benchmarks/cubism-matrix/tools/matrix.py build-native purism-native
uv run --locked python benchmarks/cubism-matrix/tools/matrix.py run purism-native
```

The current Native runner uses OpenGL. A future Metal runner should report a
different `graphics_api` and must not be merged into the OpenGL baseline.

## Core-only cases

These cases exclude Cubism Framework and graphics APIs. The startup
samples are deliberately bounded because the public Core ABI has no destroy
function and compatible providers may keep parsed state outside caller-owned
in-place buffers. The 750 ms steady-state phases are the primary comparison.

```sh
uv run --locked python benchmarks/cubism-matrix/tools/matrix.py build-core cubism-core
uv run --locked python benchmarks/cubism-matrix/tools/matrix.py run cubism-core

uv run --locked python benchmarks/cubism-matrix/tools/matrix.py build-core purism-core
uv run --locked python benchmarks/cubism-matrix/tools/matrix.py run purism-core
```

For a repeatable comparison, build both providers, run them three times in
alternating order, and write median data to
`artifacts/results/latest-core-only.json`:

```sh
uv run --locked python benchmarks/cubism-matrix/tools/matrix.py benchmark-core --repeats 3
```

## Kasane versus official Native rendering

On macOS, build and run the Kasane case by itself:

```sh
uv run --locked python benchmarks/cubism-matrix/tools/matrix.py build-kasane kasane-metal
uv run --locked python benchmarks/cubism-matrix/tools/matrix.py run kasane-metal
```

For a repeatable comparison with the official Cubism Core and Framework Native
runner, use interleaved trials:

```sh
uv run --locked python benchmarks/cubism-matrix/tools/matrix.py benchmark-render --repeats 3
```

The command saves all trials, medians, and the Kasane/Cubism FPS ratio in
`artifacts/results/latest-render-comparison.json`. Kasane also saves its last
frame as `artifacts/results/latest-kasane-metal.png`; an empty frame is rejected.
Both cases use the same model hash, 40 instances, grid, viewport, mipmaps,
warmup, and sampling duration. Kasane's frame time includes animation, geometry
evaluation, scene assembly, Metal buffer synchronization, encoding, and GPU
completion. It renders to an offscreen target. The official Native case updates
the Cubism runtime and presents an OpenGL window. Interpret the reported ratio
as a stack-level comparison; the two graphics APIs and presentation paths differ.
This is **not a matched rendering workload**: Native hardcodes model height
`0.44` and vertical offset `1.27`, whereas Kasane fits the canvas to `cell_fill`.
Native swaps with VSync disabled but does not explicitly wait for GPU completion;
Kasane waits after every frame. Native uses the Framework's default 256×256 mask
buffer; Kasane sizes masks at canvas-pixel density with padding. Native selects
an Idle motion (Mao currently has only Idle index 0), rather than reading the
configured motion. The `model_hash` hashes only model3.json, not all referenced
assets. The comparison validator checks instance count, viewport, mipmaps, and
that hash; it does not establish pixel equivalence or equal mask quality.

Use unchanged Kasane workload/settings for optimization acceptance. The
[Metal optimization audit](results/historical/2026-09-29-kasane-metal-optimized.md)
records the 20 FPS target and before/after measurements.

## Measurement rules

- Use Release builds, VSync off, the same model fixture and viewport.
- Generate texture mipmaps in both stacks and use linear mipmap filtering.
- Run compatibility checks before collecting performance results.
- Execute every case multiple times in an interleaved order.
- Compare medians and spread, not a single best run.

### Animation and geometry profiling

The Kasane result separates `p50_animation_update_ms` (Motion, Expression,
Physics, Pose and rescheduling) from `p50_geometry_evaluation_ms` (drawable
geometry, Part opacity and model opacity). The combined
`p50_animation_evaluation_ms` remains the enclosing end-to-end phase. Each
preview still evaluates independently and serially.

`p50_geometry_breakdown_ms` reports the per-frame sum across all previews for
workspace setup, core preflight, parameters, keyform selections, Parts,
transforms, meshes, Glue, render-plan construction, Part opacity, and model
opacity. `unattributed` is the remaining time in the enclosing geometry phase,
including frame handoff and timer overhead. Each field is independently reduced
to p50, so the reported p50 fields need not add up exactly to
`p50_geometry_evaluation_ms`. The ordinary `evaluate_drawables` path does not
read these timers; the matrix runner calls `evaluate_drawables_timed`.

For deterministic CPU output comparison across code revisions:

```sh
cargo run --release --locked -p cubism-matrix-kasane --example evaluation_trace -- \
  models/local/mao/runtime/mao_pro.model3.json /tmp/evaluation-trace.jsonl
```

This writes 600 snapshots and frames with a fixed irregular delta sequence,
including reset, seek and periodic hidden-geometry evaluations. Compare traces
from the same model package with `cmp`. Serialization is outside the reported
CPU timings; this diagnostic is not the end-to-end FPS benchmark.
