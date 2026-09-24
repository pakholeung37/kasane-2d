# Kasane Python SDK

`kasane` is the distributable Python interface for Kasane 2D authoring. It
creates and edits 2D model projects, evaluates parameterized geometry, imports
model3/MOC3 input, and exports MOC3 packages. The optional `observe` build adds
offscreen WGPU rendering. The Rust crates remain available for Rust callers;
the Python package is the wheel intended for Python SDK users.

The current wheel targets **CPython 3.14** (`>=3.14,<3.15`). The repository
builds and validates wheels, but does not contain a package publication
workflow. Install a wheel built from this repository; do not assume a package
on an index is this build.

## Build and install

From the repository root:

```sh
uv sync --locked
uv build --wheel --python 3.14 --out-dir target/python-wheels modules/kasane-python
uv pip install --python .venv /absolute/path/to/kasane-wheel.whl
```

Replace the wheel path with the actual file produced by the build command.
Use `target/python-wheels` for final wheels: Maturin stages its build in
`target/wheels`, so using that same path as `uv build --out-dir` causes a
same-file copy error.
The repository uses uv and the committed root `uv.lock` for Python tools.
Outside the repository, create an environment with `uv venv --python 3.14`
and install the wheel with `uv pip install --python .venv /absolute/path/to/kasane-wheel.whl`.
After installing a wheel into the repository environment, `uv run --locked`
retains it; an exact `uv sync --locked` removes packages not in the root lock,
so reinstall the wheel if needed.
The workspace release profile disables debug-info stripping only for
`kasane-python`. This keeps the wheel loadable on macOS 27 with a Homebrew
Rust build linked against an external `llvm@22` whose `llvm-objcopy` still
produces misaligned Mach-O `LINKEDIT` data. Rust upstream [fixed this
alignment bug](https://github.com/rust-lang/rust/pull/158410) in its own LLVM
build. No environment variable is needed when building this repository.
The default build supports CPU authoring and evaluation. For GPU observation,
build with Maturin's `observe` feature and install that wheel instead:

```sh
uv run --locked maturin build --manifest-path modules/kasane-python/Cargo.toml --release --features observe --out target/python-wheels
```

The GPU build still needs a working graphics device at runtime. Query
`kasane.capabilities()["gpu_observation"]` before using `kasane.Observer`.

## First project

Save this as `first_project.py`. Supply an existing PNG and an output directory
using **absolute paths**. The SDK reads the PNG when adding the asset; keep it
available until the project is saved.

```python
from pathlib import Path
from sys import argv
from uuid import uuid4

import kasane

texture = Path(argv[1]).resolve(strict=True)
output = Path(argv[2]).resolve()
document_id, asset_id, mesh_id = (str(uuid4()) for _ in range(3))

session = kasane.Session(document_id, 100, 100, (50, 50), 10)
with session.edit("create face") as edit:
    edit.add_png_asset(asset_id, "face texture", texture)
    edit.create_rectangle(mesh_id, "face", asset_id, (40, 40), (60, 60))

print(session.mesh(mesh_id).name)
print(session.evaluate({}).drawables[0].positions)
saved = session.save(output / "project")
print(saved.manifest)  # The actual project manifest path.

reopened = kasane.open_project(saved.manifest)
print(reopened.mesh_ids())
```

Run `uv run --locked python first_project.py /absolute/texture.png /absolute/output`.
`Session.save()` does not overwrite an unrelated project. For a new numbered
destination when one exists, pass `on_exists="new"` and use the returned
`SaveResult.manifest`.

## Parameterized mesh

Add a parameter and its complete mesh binding in one edit. This example
continues from the project above, before or after saving:

```python
parameter_id, binding_id = str(uuid4()), str(uuid4())
base = session.mesh(mesh_id).positions
shifted = [(x + 10, y) for x, y in base]

with session.edit("add expression") as edit:
    edit.create_parameter(parameter_id, "Open", 0, 1, 0)
    edit.create_mesh_binding(
        binding_id,
        mesh_id,
        [kasane.Axis(parameter_id, [0, 1])],
        [kasane.MeshKeyform([0], base), kasane.MeshKeyform([1], shifted)],
    )

frame = session.evaluate({"Open": 0.5})
print(frame.parameters[0].value, frame.drawables[0].positions)
```

`evaluate()` leaves the session unchanged. Parameter values may use an ID or a
unique display name. If names collide, use IDs. `evaluate_snapshot()` returns
the full drawable and render-plan data; `evaluate()` returns a smaller geometry
view.

## Import and export

```python
session = kasane.Session(str(uuid4()), 100, 100, (50, 50), 10)
result = session.import_model3(Path("/absolute/model.model3.json"))
print(result.moc_version, result.diagnostics, result.warnings)

saved = session.save(Path("/absolute/output/project"))
published = session.export_package(Path("/absolute/output/package"))
print(saved.manifest, published.published, published.warnings)
```

For a standalone MOC3, use `import_bare_moc3(moc3_path, {slot: png_path})` with
an absolute PNG path for each texture slot. Import replaces the session's
current project. Inspect `ImportResult.diagnostics` and
`session.diagnose_resources()` for missing or damaged textures.

For layered 8-bit RGB PSD artwork, import into a new project directory:

```python
result = session.import_psd(
    Path("/absolute/art.psd"), Path("/absolute/output/art-project")
)
print(result.manifest, result.raster_layers, result.warnings)
```

The import publishes the project manifest and cropped PNG assets together, then
replaces the session. The destination must not already exist. Unsupported PSD
features fail the import without changing the current session.

Imported PSD layers start as four-vertex rectangles. To add an editable grid
to one layer, including one that already has mesh keyforms, mesh BlendShape
deltas, or corner glue, use:

```python
mesh = session.require_unique_mesh("ArtMesh24")
session.remesh_rectangle_grid(mesh.id, 12, 12)
```

The operation retains corner vertex IDs and migrates existing dependencies in
one undoable edit. Bound meshes require equal column and row counts. For a new
binding created in the same edit, `kasane.rectangle_grid_geometry()` provides
the grid without opening a separate edit. The one-off
[`Shirousagi experiment`](../../docs/experiments/shirousagi_head_x.py) records
a real-PSD run, including its reference-model displacement projection.

## GPU observation

With an `observe` wheel and `gpu_observation` capability, a saved project can
be rendered as follows:

```python
session = kasane.open_project(Path("/absolute/output/project"))
with kasane.Observer(512, 512, 512) as observer:
    frame = observer.observe(session, {})
    frame.save_png(Path("/absolute/output/preview.png"))
    run = observer.observe_run(
        session, [{}], Path("/absolute/output/runs"),
        focus=session.mesh_ids(),
    )
    print(run.report, run.contact_sheet)
```

Add parameter samples to the `observe_run()` list as needed. The call creates
a unique directory containing rendered frames, a contact sheet, and a JSON
report.

## Errors and scripting

SDK validation and IO errors raise `kasane.SdkFailure`. Inspect `code`,
`operation`, `object_ids`, `field_path`, `expected_version`, `actual_version`,
and `referrers` instead of parsing the message. A failed command aborts its
whole edit, even if the exception is caught inside the `with` block. Leaving
the block with an exception also rolls the edit back. GPU errors raise
`kasane.ObservationFailure` with `code` and optional `asset_id`.

To run a script once and write a machine-readable result:

```sh
uv run --locked python -m kasane run /absolute/script.py --report /absolute/report.json
```

The runner records output, exceptions, and session versions. It does not save
projects automatically; scripts must call `Session.save()` themselves.

For method groups, data records, and behavior details, see the
[Python API reference](https://github.com/pakholeung37/kasane-2d/blob/main/modules/kasane-python/API.md).
For repository validation, see the
[validation guide](https://github.com/pakholeung37/kasane-2d/blob/main/docs/VALIDATION.md).
