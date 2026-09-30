# Kasane geometry evaluation breakdown — 2026-09-30

The 40-model Mao Metal benchmark was run three times after adding stage timers
to `FrameEvaluator` and `MotionPreview`. Each result is a p50 across frames of
the sum of 40 serial preview evaluations. The table reports the median of the
three run-level p50s. Independent p50 values do not add exactly.

Apple M4, arm64, Rust 1.98.1, Release build, source base `4538a00` plus the
working-tree Metal synchronization optimization and this timing change. The
workload retained its 10×4 layout, 1280×720 target, mipmaps, five-second
warmup, fifteen-second sample, GPU-completion wait, 6,920 draw calls, 520 masks,
and three render passes. Import reported 16 unresolved references on each run.

| Geometry stage | Median p50, ms |
| --- | ---: |
| Mesh interpolation, BlendShape, parent deformation | **7.767** |
| Deformer evaluation | **4.147** |
| Render order and command plan | 1.193 |
| Glue | 0.762 |
| Parameter evaluation | 0.675 |
| Part opacity | 0.315 |
| Part evaluation | 0.263 |
| Preview workspace setup | 0.125 |
| Keyform selection | 0.079 |
| Core preflight | 0.071 |
| Model opacity | 0.002 |
| Unattributed handoff and timer overhead | 0.007 |
| **Enclosing geometry phase** | **15.385** |

Meshes and deformers together account for about 77% of the enclosing geometry
phase. The mesh stage includes interpolation, BlendShape updates, and application
of parent transforms, so this measurement does not isolate those three costs.
The render-plan stage builds commands and sort order on the CPU; it does not
include Metal command encoding or GPU drawing.

| Trial | Geometry p50, ms | Mesh p50, ms | Transform p50, ms | Frame p50, ms | Average FPS |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 | 15.792 | 7.980 | 4.215 | 34.395 | 28.264 |
| 2 | 15.385 | 7.767 | 4.147 | 34.043 | 27.453 |
| 3 | 15.138 | 7.590 | 4.025 | 33.762 | 27.035 |

The timers add instrumentation overhead, so these frame rates should not be
treated as an optimization comparison with earlier uninstrumented results.
Reproduce with a local Mao package:

```sh
cargo build --release --locked -p cubism-matrix-kasane
target/release/cubism-matrix-kasane \
  benchmarks/cubism-matrix/config/mao-40.json \
  "$(pwd)/models/local/mao/runtime/mao_pro.model3.json"
```
