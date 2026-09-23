# Persistent scene plan benchmark — 2026-09-23

Same-session sequential comparison of the archived de62a6b Release library against this refactor, before then after for each workload. Library hashes and source hashes are in metadata.json. The original benchmark scripts and measurement API were unchanged. Three repetitions per variant; median of each per-run statistic. Fixture: 60 warmup / 600 samples; Ren: 30 warmup / 120 samples at 2048². Existing Godot editor left open.

| Metric | Before (ms) | After (ms) | Change |
| --- | ---: | ---: | ---: |
| gpu-fixture refresh_cpu_ms.mean | 0.461252 | 0.448315 | -2.80% |
| gpu-fixture refresh_cpu_ms.p95 | 0.717000 | 0.696000 | -2.93% |
| gpu-fixture frame_ms.mean | 8.334460 | 8.343307 | +0.11% |
| gpu-fixture frame_ms.p95 | 14.620000 | 14.714000 | +0.64% |
| ren-offscreen refresh_cpu_ms.mean | 2.814058 | 2.744758 | -2.46% |
| ren-offscreen refresh_cpu_ms.p95 | 3.375000 | 3.151000 | -6.64% |
| ren-offscreen frame_ms.mean | 11.907967 | 11.625358 | -2.37% |
| ren-offscreen frame_ms.p95 | 15.159000 | 14.212000 | -6.25% |

Both workloads pass the existing 5% gate. Texture memory, mask/offscreen count, offscreen creation/resize and upload counts match. Ren first-run video memory +0.73%; this is engine-reported process memory, not total driver memory. Small timing differences remain subject to desktop scheduling noise. CPU refresh includes evaluation, renderer synchronization and the full frame Dictionary returned by set_preview_values; frame time includes scheduling and rendering.

GPU contracts separately verify that camera changes do not increment scene_submissions or geometry_syncs, mask viewport identity survives density changes within a sampling policy, and texture replacement no longer rebuilds geometry. The existing parameter workloads do not measure camera-only CPU timing.

Validation: targeted GPU suites 23 + 106 + 34 checks and destination-copy positive/negative controls; Godot integration 188 checks; official GPU comparison 84 checks. Core/render/preview Rust tests and renderer Clippy passed. Runtime wgpu implementation was not changed or validated in this round.
