# Renderer extraction benchmark — 2026-09-23

Baseline: c21c111; current: de62a6b. Both clean Release builds, same PurismCore ad5127666ccffb34552d8e660852995858bee437, rustc 1.98.1, Godot 4.7.2 Mono, GL compatibility. Same current benchmark scripts used for both libraries. Existing Godot editor left open. Runs executed sequentially, baseline then current per workload.

Each variant repeated 3 times. Fixture: 60 warmup + 600 sampled frames; Ren: 30 warmup + 120 sampled frames, 2048x2048 target, 24 offscreens. Time values are medians of per-run statistics; frame time is wall-clock through frame_post_draw, not isolated GPU time.

| Workload / metric | Before (ms) | After (ms) | Change |
| --- | ---: | ---: | ---: |
| Fixture CPU mean | 0.486480 | 0.502663 | +3.33% |
| Fixture CPU p95 | 0.737 | 0.746 | +1.22% |
| Fixture frame mean | 8.343753 | 8.366515 | +0.27% |
| Fixture frame p95 | 14.164 | 14.148 | -0.11% |
| Ren CPU mean | 2.715175 | 2.715083 | ~0.00% |
| Ren CPU p95 | 3.258 | 3.217 | -1.26% |
| Ren frame mean | 10.990842 | 10.920525 | -0.64% |
| Ren frame p95 | 13.376 | 12.907 | -3.51% |

Both workloads pass the existing 5% regression gate. Texture memory, masks, offscreen count/creations/resizes and upload counts match. Fixture video memory matches. The comparison script uses the first run for resource metrics: Ren video memory reports 566649001 -> 577299961 bytes (+1.88%). Across all three runs, baseline video memory is 566649001/579430153/592211305 and current is 577299961/564518809/577299961 bytes; this fluctuates and does not establish a persistent memory regression.

This run shows approximately preserved performance, not a demonstrated speedup. Raw reports, logs, comparison output, commit IDs and library SHA-256 hashes are stored alongside this summary. Runtime source was not modified.
