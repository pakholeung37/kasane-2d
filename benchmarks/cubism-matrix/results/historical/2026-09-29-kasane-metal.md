# Kasane Metal versus official Native — 2026-09-29

Apple M4, macOS 27.0, Release builds. The `mao-40` workload used the same
`mao_pro.model3.json` hash in both cases, 40 instances in a 10×4 grid,
1280×720 output, texture mipmaps, 5 seconds of warmup, and 15 seconds of
sampling. The cases ran in alternating order for three trials.

| Case | FPS trials | Median FPS | Median p50 frame time |
| --- | --- | ---: | ---: |
| Official Cubism Core + Framework Native/OpenGL | 54.604, 54.170, 54.733 | 54.604 | 17.397 ms |
| Kasane evaluation + kasane-render-metal | 8.611, 8.710, 8.693 | 8.693 | 116.195 ms |

Kasane achieved **0.159×** the official Native throughput, or **6.28×** the
frame time implied by the FPS ratio. Its rendered output was nonempty and was
visually inspected as a complete 40-model grid. The Kasane run reported 6,920
draw calls, 520 masks, and 521 render passes per frame.

Kasane's median per-frame phase measurements, taking the median of each trial's
p50, were: animation and geometry evaluation 24.206 ms, scene assembly 3.002 ms,
Metal model synchronization 9.342 ms, encoding 46.560 ms, and waiting for GPU
completion 32.333 ms. These phase medians do not necessarily sum to the median
whole-frame time because they are measured and ranked separately.

This is an end-to-end stack comparison. Kasane imports the model into an
editable document, evaluates 40 independent animation previews, combines
their drawables into one scene, and waits for an offscreen Metal command buffer.
The official runner updates 40 Cubism runtime models and presents an OpenGL
window. Graphics APIs, presentation, animation implementations, and GPU
synchronization differ, so the 6.28× figure is not an isolated renderer ratio.
Kasane's import reported 16 unresolved model3 references, primarily absent
parameter IDs in motion curves; it still rendered the complete grid.

Reproduce with:

```sh
uv run --locked python benchmarks/cubism-matrix/tools/matrix.py benchmark-render --repeats 3
```

Full per-trial JSON and the Kasane output PNG are written to the ignored
`benchmarks/cubism-matrix/artifacts/results/` directory.
