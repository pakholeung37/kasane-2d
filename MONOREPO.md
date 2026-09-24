# Runtime monorepo

## Active authoring and rendering

- `modules/kasane-core/` — editable document, deformation and evaluation.
- `modules/kasane-project/` — project persistence, resource validation and package publication.
- `modules/kasane-moc3/` — MOC3 import, export and safety checks.
- `modules/kasane-render/` — renderer-independent scene plan.
- `modules/kasane-render-wgpu/` — WGPU renderer.
- `modules/kasane-preview/` — host-independent preview resource checks.
- `modules/kasane-sdk/` — transactional Rust authoring API.
- `modules/kasane-sdk-observe/` — immutable observation and GPU capture.
- `modules/kasane-python/` — Python wheel and agent scripting interface.
- `tests/` — acceptance-gate tests and shared model, texture and image-reference fixtures.
- `models/` — local Live2D model sources and their usage inventory; small generated test fixtures stay in `tests/fixtures/`.
- `tools/` — SDK, Core and WGPU acceptance commands.

## Separate comparison projects

- `modules/purism-core/` — Cubism Core-compatible implementation.
- `modules/gd-cubism/` and `benchmarks/cubism-matrix/` — independent official Cubism/Purism comparison benchmark.
- `third_party/` — local, untracked SDKs and assets.
