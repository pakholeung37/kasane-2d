# Metal mesh synchronization follow-up — 2026-09-29

The unchanged `mao-40` workload reached **28.911 FPS median** after optimizing
Metal mesh synchronization, versus **26.072 FPS** for the freshly rebuilt `main`
baseline in this session (**10.9% higher throughput**). Median p50 Metal sync
fell from **9.392 ms to 5.445 ms**. The historical 28.219 FPS result used an
earlier run and is not the paired baseline for this measurement.

Apple M4, macOS 27.0 (26A428), Rust 1.98.1, Release builds. Baseline source
revision: `4538a00`; optimized source is the accompanying working-tree change.
Both builds used the same local Mao model package and 40 independent previews,
10×4 grid, 1280×720 output, mipmaps, five seconds of warmup, 15 seconds of
sampling, and a GPU-completion wait after every frame. No builds or tests ran
concurrently with timed samples. All final frames retained 6,920 draw calls,
520 masks, and three render passes. The complete grid output was visually
inspected.

| Case | FPS trials | Median FPS | Median p50 frame | Median p50 Metal sync |
| --- | --- | ---: | ---: | ---: |
| Rebuilt `main` | 26.103, 26.042 | 26.072 | 36.000 ms | 9.392 ms |
| Optimized CPU packing | 29.036, 27.190, 28.911, 27.920, 29.075 | 28.911 | 33.454 ms | 5.445 ms |
| GPU vertex-conversion experiment | 28.939, 28.957 | 28.948 | 33.564 ms | 5.021 ms |

After applying the optimized code to the primary checkout, an additional
full-length run there measured 28.432 FPS, 33.448 ms p50 frame time, and
5.407 ms p50 Metal sync. It is a validation run and is not included in the
trial medians above.

The optimized CPU path updates `ScenePlan` in place after its fallible
validation, removes per-mesh CPU vertex/index shadow allocations, and writes
changed geometry directly into packed upload slabs. It compares the current
frame with the previous caller-owned frame to reuse immutable Metal buffers
when positions, UVs, indices, and canvas are unchanged. New index allocations
with equal contents also reuse the existing buffer. Changing indices uploads
only the affected index slab. Metal command buffers still retain immutable
buffers until their submitted work completes.

## GPU experiment

A separate variant uploaded model coordinates and UVs as 16-byte vertices,
then applied canvas scaling, Y inversion, and UV orientation in the Metal
vertex shader. Metal pixel tests passed, and median p50 sync fell another
0.424 ms. Median p50 GPU wait rose from 2.895 ms for the optimized CPU path
to 4.230 ms for this variant; median p50 whole-frame time was 0.110 ms
longer. Its 0.037 FPS median throughput difference does not establish an
improvement. The GPU variant was removed; the
measured CPU packing optimization remains.

These phase medians are ranked independently and do not add up to the whole
frame median. GPU wait includes command submission and completion as measured
by the runner, so the phase difference alone does not establish that shader
arithmetic caused all of the added wait. The experiment indicates that this
particular GPU transfer did not improve the end-to-end score.

## Validation

`cargo test --locked -p kasane-render -p kasane-render-metal -p kasane-sdk-observe`
passed. The Metal suite checks pixels, masks, blending, queued-frame buffer
lifetimes, unchanged index allocations, and actual index edits. A final
`kasane-render-metal` smoke run passed again after removing the GPU variant.

Reproduce from a checkout containing the local Mao package:

```sh
cargo build --release --locked -p cubism-matrix-kasane
target/release/cubism-matrix-kasane \
  benchmarks/cubism-matrix/config/mao-40.json \
  "$(pwd)/models/local/mao/runtime/mao_pro.model3.json"
```

The runner requires an absolute model path. Its JSON and PNG output are saved
under the ignored `benchmarks/cubism-matrix/artifacts/results/` directory.
