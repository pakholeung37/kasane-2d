# kasane-core

C++20 editable source data, reference-aware editing and pure in-memory evaluation.
Document has no Godot, runtime model, texture resource or filesystem dependency.
Purism supplies shared keyform, Rotation/Warp and nested-direction algorithms.

```sh
# Full native suite: CMake, Ninja, C++20, libpng and Python are needed.
cmake --preset core-debug
cmake --build --preset core-debug
ctest --preset core-debug

# In-memory core only, when libpng is unavailable:
cmake --preset memory-core
cmake --build --preset memory-core
ctest --preset memory-core
```

- `model.hpp`: Canvas, ImageAsset, Part, Mesh, Transform (Rotation/Warp), Parameter,
  MeshBinding and SceneBinding with explicit complete Cartesian Keyforms.
- `document.hpp`: identity, CRUD, atomic geometry/Keyform updates, reference-aware
  deletion, cycle checks, object changes and revisions.
- `evaluation.hpp`: pure `evaluate_frame` returning DrawableFrame. Temporary
  parameter values stay outside persistent source data.
- `moc3.hpp` / `kasane_moc3`: compile source into MOC3 v5 and resource descriptions.
  No Core ABI or filesystem dependency.
- `package.hpp` / `kasane_package`: libpng validation, a required application
  runtime-validation callback and staged whole-directory publication.
- `legacy_deformer.hpp`: explicitly isolated prototype data. Formal evaluation
  and export reject legacy deformers, requiring explicit migration.

Root positions use canvas pixels. Rotation children use local runtime units;
Warp children use normalized grid coordinates, including extrapolation outside
`[0,1]²`. Reparenting does not implicitly transform source geometry.

The core does not own an undo stack. See [coordinate/format mapping](../../docs/editor/formats/MOC3-WRITER.md),
[API boundaries](../../docs/editor/M1-CORE-REFACTOR.md) and
[M1 acceptance](../../docs/editor/M1-ACCEPTANCE.md) for reproducible tests and limits.
The full native suite uses Purism for MOC3 evaluation and does not need the proprietary SDK.
An official Core comparison remains a separate acceptance gate.

```sh
python3 tools/validate_core.py
# Godot and GPU regressions use tools/validate_godot.py and tools/validate_gpu.py.
```
