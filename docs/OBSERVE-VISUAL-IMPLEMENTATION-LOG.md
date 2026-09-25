# Observe visual implementation log

2026-09-25. Implementation started from base commit
`768f129506baf357c81fef9e98cb6a0099cbb994`. The user's preexisting
staged research/plan changes were preserved. Fixture manifest SHA-256:
`a971cfc4a93a55c3a335ae35df8f425968f4e5490dbe3ce1c333a0eacf530331`.

| Stage | Status | Delivered here | Remaining gate |
| --- | --- | --- | --- |
| O0 contract and baseline | complete | Typed target contract, three generated/verified synthetic projects, raw pixel/timing/RSS baseline, derived light/dark arithmetic reference, edit task definition | Renderer-native background results belong to V01 implementation |
| O1 frozen capture and ROI slice | in progress | `ResolvedObservation` freezes evaluated frame, authoring mesh/Part/transform/binding records and decoded textures; explicit ROI rerender; data-only scene save/open; Python static and animation frame capture; stale-preview and Model opacity policy; independent-process reopen; canonical scene/render digests and persistent capture ID | Full packet/report v2, scene/analysis/report profiles, multi-sample snapshot, playback recipe/operation identity, public `inspect*`/`render(packet)` APIs |
| O2–O7 | not started | — | V01–V07 presentation/query/diagnostics, V08 runner, V09 traces, V10 continuity, agent experiments |

The new Python O1 API is `capture_scene`, `capture_animation_scene`,
`render_scene`, `save_scene`, and `open_scene`; it is documented in
`modules/kasane-python/API.md`. The saved scene has no resumable animation
runtime state. `CapturedScene.source` records the current animation snapshot,
host Model opacity policy and `history_status=not_recorded`; events in that
snapshot cover only the last update. `CapturedScene.authoring` is frozen
metadata and source topology, not selected-keyform provenance.

## Verification record

| Command/evidence | Result |
| --- | --- |
| `build_fixtures.py --verify` with final GPU wheel | passed, 9 files and 3 projects |
| `cargo test -p kasane-sdk-observe --locked` | passed: 4 unit + 5 integration tests; canonical digest normalization/nonfinite rejection, cross-process scene reopen, corruption/version/path rejection |
| `cargo test --workspace --locked` | passed after canonical identity work |
| `cargo test -p kasane-moc3-psd --locked` | passed after those test lint edits |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | passed after canonical identity work |
| `cargo fmt --check`, `git diff --check` | passed |
| release GPU wheel, isolated CPython 3.14 environment | passed 6 `test_observe.py` tests, including Pose opacity, digest equality and bundle reopen |
| release CPU-only wheel, isolated CPython 3.14 environment | imported without GPU observation dependency; passed 50 `test_cpu.py` tests |
| raw baseline on final GPU wheel | all five self-authored case raw hashes matched the pre-change wheel on Apple M4/Metal, including after canonical identity work |

Baseline values and reproducible commands are in
`docs/OBSERVE-VISUAL-BASELINE.md`; contract and capability limits are in
`docs/OBSERVE-VISUAL-CONTRACT.md`. The original `observe()` return tuple,
`observe_run()` report v1 and raw pixel hashes were preserved. The ROI path
still emits a provisional legacy `input_sha256` extension for compatibility;
the separate canonical `scene_digest` and `render_digest` are versioned input
identifiers. They do not promise byte-identical output across GPU adapters.
The scene bundle writer now emits schema v2 with a persisted capture ID and
validated scene digest; the reader also accepts v1 and assigns it a new ID.
