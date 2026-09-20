# Runtime monorepo

## Layout

- `apps/editor/` — standalone Godot Editor shell; modeling backends pending.
- `apps/viewer/` — independent Godot Viewer shell; runtime integration pending.

- `modules/purism-core/` — Cubism Core compatible implementation.
- `modules/gd-cubism/` — `gd_cubism` Godot GDExtension, an alternate Cubism Godot implementation.
- `modules/kasane-core/` — Rust core document model, deformers, geometry and evaluation.
- `modules/kasane-godot/` — Rust GDExtension bindings for Godot and live preview.
- `modules/kasane-moc3/` — Rust MOC3 binary encoder and purism verification.
- `modules/kasane-project/` — Rust project persistence, resource validation, atomic locks and package publication.
- `tests/` — repository-wide Godot integration tests, lifecycle boundary tests, full authoring workflow, and visual regression suites.
- `demos/gd-cubism-demo/` — interactive Godot comparison and regression demo.
- `benchmarks/cubism-matrix/` — reproducible benchmark matrix between official
  Cubism Core and PurismCore.
- `third_party/` — local, untracked third-party SDKs shared by modules and apps.
- `tools/` — repository-wide development and staging utilities.
