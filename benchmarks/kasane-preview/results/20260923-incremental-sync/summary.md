# Incremental model synchronization — 2026-09-23

Before: archived phase-one ScenePlan library (`fe07e9c…`), rerun in this session.
After: working tree based on `4471373`, adding immutable topology validation reuse,
drawable material value comparison and native node-order comparison. Exact library
and source hashes are in `metadata.json`; scene.rs subsequently received a documentation-only comment edit.
No wgpu implementation changes.

Apple M4, Godot 4.7.2 Mono, GL compatibility, Release. Each variant ran sequentially
three times. Fixture: 60 warmup / 600 sampled frames. Ren: 30 / 120, 2048²,
24 offscreen groups and 8 mask viewports. Values below are medians of per-run statistics.

| Workload | CPU mean before → after | CPU p95 change | Frame mean before → after | Frame p95 change |
|---|---:|---:|---:|---:|
| Fixture | 0.455620 → 0.451842 ms (-0.83%) | +2.26% | 8.446377 → 8.573232 ms (+1.50%) | +2.38% |
| Ren | 2.627533 → 2.372767 ms (-9.70%) | -7.18% | 10.917258 → 10.477367 ms (-4.03%) | -6.74% |

Both pass the existing 5% mean/p95/resource gate. Fixture frame p50 increases 7.85%
(diagnostic, not gated). Desktop timing remains noisy; CPU includes evaluation and
full Dictionary construction, and frame time includes host scheduling.

Texture memory, mesh uploads and main resource counts are unchanged. Ren records
121 model submissions, 1 node-order sync and 221 drawable material syncs across
198 meshes; the previous implementation visited all materials and reordered nodes
every submission. Explicit texture refresh still redraws raw-alpha masks.

Validation: 77 Rust tests, renderer Clippy, GPU image comparison (84 checks),
render boundary suites (174 checks plus destination-copy positive/negative control)
and Godot integration (188 checks). See the adjacent validation reports.
Further work remains on typed internal geometry upload and offscreen material
updates; this is not a claim of zero-allocation animation or global optimality.
