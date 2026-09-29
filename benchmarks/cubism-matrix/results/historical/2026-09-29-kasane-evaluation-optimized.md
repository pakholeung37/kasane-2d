# Animation and geometry evaluation optimization — 2026-09-29

After the [Metal optimization](2026-09-29-kasane-metal-optimized.md), the same
40-model workload now reaches **28.219 FPS** (median of three runs).
The fresh pre-change run was **24.882 FPS**, so end-to-end throughput
improved by **13.4%**. Animation plus geometry
p50 fell by **19.0%**.

Apple M4, macOS 27.0, Release builds. Same `mao-40` model, 40 independent serial
previews, 10×4 grid, 1280×720, mipmaps, 5-second warmup and 15-second samples.
Every frame still waits for Metal GPU completion. The earlier fairness caveats
for the Native comparison remain; this report compares Kasane to itself.
The baseline includes the previous Metal optimization, not the old 8.693 FPS
renderer. Before is one fresh run; after is three consecutive runs. No builds,
tests, CPU profiling or trace serialization ran during final timed sampling.

| Measurement | Before p50 | After median of p50s | Reduction |
| --- | ---: | ---: | ---: |
| Animation update | 3.500 ms | 2.644 ms | 24.5% |
| Geometry evaluation | 17.556 ms | 14.535 ms | 17.2% |
| Combined animation + geometry | 21.229 ms | 17.196 ms | 19.0% |
| Whole frame | 39.785 ms | 34.662 ms | 12.9% |

Final FPS trials: 28.219, 27.777, 28.270.
Final p95 frame times: 40.646, 41.487, 40.306 ms.
Phase medians are ranked independently and do not sum to the whole-frame median.
Animation timing includes scheduling; geometry timing includes Part/model opacity.
All trials retain **6,920 draw calls, 520 masks and 3 render passes**.

## Changes

1. **Reuse preview scratch storage.** `MotionPreview::evaluate_drawables` keeps
   a private `FrameEvaluator` and parameter map. Transform buffers, selections,
   temporary maps and parameter UUID keys survive calls. Returned frames remain
   owned by the caller and are fully evaluated each time; there is no pose reuse
   between models. A mutex preserves the existing shared-reference API and
   `Send + Sync`; concurrent geometry queries on one preview serialize, while
   independent previews have independent workspaces. Seeking may replace scratch
   storage. The hidden-geometry observation path retains its existing semantics.
2. **Batch parent deformation.** Mesh vertices and child-warp control points now
   pass through one batch operation. Rotation coefficients are computed once per
   batch; warp extrapolation prepares its basis at most once when needed. This
   eliminates repeated single-point function setup without unsafe casts, extra
   vertex buffers, approximate math or changed arithmetic order.
3. **Prepare and share keyframe selection grids.** The document's prepared
   evaluation data interns identical axis grids, resolves parameter UUIDs to
   numeric slots, and stores the original snapping epsilon. Each unique grid is
   evaluated once per frame using normalized parameter values. Meshes, transforms,
   Parts, offscreens and Glue read the resulting indices/weights. Structural,
   parameter and binding edits invalidate prepared data; current keyform positions
   are still read on each evaluation. No time quantization or frozen-frame cache
   was introduced.
4. **Reuse animation parameter maps.** Motion stages copy values into existing
   tree nodes when UUID keys agree. Physics writes update existing values without
   cloning keys. Key-set changes fall back to full replacement, including removal
   of stale entries. Part-opacity maps use the same value-copy path. Motion,
   Expression, Physics and Pose retain their original order and timestep rules.

## Validation

`cargo test --locked -p kasane-core -p kasane-animation -p kasane-sdk -p kasane-sdk-observe`
passed **148 tests**. New/extended coverage checks compiled selections against the
scalar Cartesian reference; parameter precision, axis edits and document restore;
caller-owned frame independence; shared preview reads; unchanged snapshot/operation
identity; and parameter-map key replacement and signed zero.

A CPU-only trace generated before the changes was compared with the final version
using `cmp`. All **600 frames and animation snapshots were byte-identical**, including
irregular fixed deltas, zero-delta updates, a backwards seek, reset, loop traversal,
and periodic hidden-geometry evaluation. These traces include geometry and metadata,
not just screenshots or a visible-pixel count. The model import still reports the
same 16 unresolved references; this is equivalence to prior Kasane behavior, not
new evidence of exact official Cubism parity.

Trace SHA-256: `0038b4fdd1784ef13db51077502daafccf905ccb16797ecb660750db57bddc2d`

The trace is 500,067,586 bytes for this local Mao package and is not
committed. Reproduce it with:

```sh
cargo run --release --locked -p cubism-matrix-kasane --example evaluation_trace -- \
  models/local/mao/runtime/mao_pro.model3.json /tmp/evaluation-trace.jsonl
```

To reproduce FPS measurements, build once and run the case three times serially:

```sh
uv run --locked python benchmarks/cubism-matrix/tools/matrix.py build-kasane kasane-metal
uv run --locked python benchmarks/cubism-matrix/tools/matrix.py run kasane-metal
```

[Raw measurements](2026-09-29-kasane-evaluation-optimized.json) retain the fresh
baseline, all three final trials and the trace digest. The CPU trace's own timings
exclude serialization but are not used as the end-to-end benchmark result.
