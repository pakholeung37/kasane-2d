# Kasane Metal optimization and fairness audit — 2026-09-29

The unchanged Kasane `mao-40` workload reaches **23.853 FPS median** across
three runs, exceeding the 20 FPS average-throughput target in every run.
This is **2.55×** the fresh pre-change run and
**2.74×** the historical 8.693 FPS median. It is not a guarantee that
every frame finishes within 50 ms.

Apple M4, macOS 27.0 (26A428), Rust/Cargo 1.98.1, Release builds. Base source
revision: `cf79382`; results include the accompanying working-tree optimization.
40 independent editable-model animation previews are still evaluated serially,
assembled into a 10×4 scene, rendered at 1280×720 with mipmaps, and waited on
until the Metal command buffer completes. Warmup is 5 seconds and sampling is
15 seconds. No model deduplication, animation freezing, density reduction,
parallel evaluation, or asynchronous frame timing was used.

## Fairness findings

The original comparison is useful as an end-to-end stack reference, but is
**not a controlled renderer comparison**:

- Kasane waits for GPU completion each frame; Native calls `glfwSwapBuffers`
  with VSync disabled and no explicit `glFinish`. Submission/presentation and
  completed-frame latency are not equivalent timing boundaries.
- Native hardcodes height `0.44` and vertical placement offset `1.27` in
  `LAppLive2DManager.cpp`. Kasane fits the model canvas using `cell_fill=0.92`.
  A shared 10×4 grid does not establish equal projected model size or fill cost.
- Native uses the Framework's default 256×256 clipping-mask buffer and packing
  policy. Kasane retains its canvas-pixel mask density and four-pixel padding.
  Mipmaps on both sides do not imply equal masking quality or cost.
- Native selects an Idle motion rather than consuming the configured motion
  group/index. The current Mao package has only Idle index 0, so the present
  motion asset agrees, but the runner would not honor arbitrary motion changes.
  Animation, physics and procedural update implementations still differ.
- The shared hash covers `mao_pro.model3.json` only, not its referenced moc,
  textures, motions, pose or physics files. The tool's equality checks do not
  establish pixel equivalence or package-wide identity.
- Kasane continues to report 16 unresolved import references, primarily missing
  parameter IDs. The complete 40-model output was visually inspected; this is
  not a claim of exact Cubism animation parity.

These Native settings were left unchanged to preserve the historical reference.
Acceptance is based on **Kasane versus its own unchanged workload**, rather
than the Kasane/Native ratio. The original report remains intact.

## Measurements

| Case | FPS trials | Median FPS | Median p50 frame time |
| --- | --- | ---: | ---: |
| Kasane, historical report | 8.611, 8.710, 8.693 | 8.693 | 116.195 ms |
| Kasane, fresh pre-change run | 9.369 | 9.369 | 106.132 ms |
| Kasane, optimized | 23.050, 23.853, 23.854 | 23.853 | 41.766 ms |
| Official Native, contextual reference | 55.692, 55.207, 53.621 | 55.207 | 17.399 ms |

Optimized p95 frame times: 50.953, 45.891, 46.473 ms.
The final cases ran in alternating order across three trials; builds and tests
were not run concurrently with timed sampling.

| Phase | Fresh pre-change p50 | Optimized median of p50s |
| --- | ---: | ---: |
| Animation + geometry evaluation | 22.588 ms | 22.236 ms |
| Scene assembly | 2.924 ms | 3.073 ms |
| Metal synchronization | 8.567 ms | 9.710 ms |
| Metal encoding | 46.344 ms | 4.504 ms |
| GPU completion wait | 25.295 ms | 1.669 ms |

Phase medians are ranked independently and do not sum to whole-frame medians.
The fresh pre-change result is one run; the historical and optimized medians
have three trials each. Wall-clock animation advances differ between runs,
so final visible-pixel counts are coverage checks, not image-equivalence checks.

## Implementation

Metal now uses `ScenePlan`'s mesh index and previously computed mask bounds,
removing repeated full-scene ID searches and geometry walks during encoding.
Large scenes pack masks into atlas shelves at the original mask dimensions.
Scissor rectangles isolate writes; tile-local texel clamping prevents sampling
neighboring masks. Unchanged atlases are cached, changed ones are rebuilt as a
unit, and same-size pages are reused with same-queue ordering. Small scenes keep
individual-mask caching. The attachment budget includes shelf waste; a budget
failure falls back to individual masks.

Each measured optimized frame still has **6,920 draw calls and 520 masks**.
Render passes fall from **521 to 3** (two atlas pages and the output pass).
The CLI also drains autoreleased command-buffer objects instead of retaining
them for the process lifetime and reports its synchronization/layout/mask policy.

## Validation and reproduction

- `cargo test --locked -p kasane-render -p kasane-render-metal -p kasane-sdk-observe`:
  all 46 tests passed, including scene validation, masking, blending, nested
  offscreens, buffer lifetime and SDK observation integration.
- Added pixel-for-pixel comparison of 40 atlas masks against independent masks,
  including fractional placement, inverted masks, padding and tile edges.
- Added queued-frame coverage for static cache reuse, geometry edits, texture
  revision invalidation and transition below the atlas threshold.
- Complete 40-model output visually inspected after optimization.

```sh
uv run --locked python benchmarks/cubism-matrix/tools/matrix.py benchmark-render --repeats 3
```

[Raw measurements](2026-09-29-kasane-metal-optimized.json) retain all final trials
and the fresh pre-change result in the repository. The generated screenshot and
latest runner outputs remain in ignored `artifacts/results/`.
