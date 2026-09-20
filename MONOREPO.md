# Runtime monorepo

## Layout

- `apps/editor/` — standalone Godot Editor shell; modeling backends pending.
- `apps/viewer/` — independent Godot Viewer shell; runtime integration pending.

- `modules/purism-core/` — forked PurismCore provider, pinned as a Git
  submodule.
- `modules/gd-cubism/` — `gd_cubism` Godot GDExtension, imported from
  `gd_cubism` and adapted to support an alternate Cubism Core implementation.
- `modules/kasane-core/` — Rust core document model, deformers, geometry and evaluation.
- `modules/kasane-godot/` — Rust GDExtension bindings for Godot, live preview, and GDScript test suites.
- `modules/kasane-moc3/` — Rust MOC3 binary encoder and purism verification.
- `modules/kasane-project/` — Rust project persistence, resource validation, atomic locks and package publication.
- `demos/gd-cubism-demo/` — interactive Godot comparison and regression demo.
- `benchmarks/cubism-matrix/` — reproducible benchmark matrix between official
  Cubism Core and PurismCore.
- `third_party/` — local, untracked third-party SDKs shared by modules and apps.
- `tools/` — repository-wide development and staging utilities.
