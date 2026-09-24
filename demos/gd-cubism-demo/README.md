# gd_cubism integration demo

A minimal end-to-end Godot 4 demo using the `gd_cubism` GDExtension and
Cubism Native Framework with the local Nijiiro Mao sample model.

## Local prerequisites

The model and the proprietary Cubism Native SDK cannot be redistributed in
this repository. Place them at these paths from the monorepo root:

```text
demos/gd-cubism-demo/assets/live2d/mao/
third_party/CubismSdkForNative-5-r.5/
```

Initialize `godot-cpp` and prepare SCons once:

```sh
git submodule update --init --recursive
python3 -m venv modules/gd-cubism/.venv
modules/gd-cubism/.venv/bin/python -m pip install scons==4.7.0
```

## Run

Open `demos/gd-cubism-demo/project.godot` with Godot 4.7 or newer, or launch it from
the monorepo root. This project selects Purism Core by default through
`gd_cubism_provider.txt` when its addon is staged:

```sh
/Applications/Godot_mono.app/Contents/MacOS/Godot \
  --path demos/gd-cubism-demo

/Applications/Godot_mono.app/Contents/MacOS/Godot \
  --headless --path demos/gd-cubism-demo \
  --script res://tests/smoke_test.gd
```

## PurismCore through the Cubism Core ABI

`modules/purism-core` provides PurismCore through the Cubism Core C ABI expected
by the native framework. Run its complete build/stage/test cycle from the
monorepo root:

```sh
tools/run_cubism_core_experiment.sh
```

To launch the interactive demo with the PurismCore provider selected:

```sh
tools/run_cubism_core_demo.sh
```

Provider-specific extension binaries remain together in the canonical addon's
`bin/` directory and are reused by later runs.

## Rebuild the `gd_cubism` addon

```sh
cd modules/gd-cubism
.venv/bin/python -m SCons platform=macos arch=arm64 target=template_debug -j8
cd ../..
python3 tools/stage_godot_addon.py demos/gd-cubism-demo
```

Set `CUBISM_CORE_PROVIDER` together with `CUBISM_CORE_LIBRARY` when building an
alternate provider. Select that existing binary while staging with
`--core-provider purism` or `--core-provider cubism`; the other provider
binaries are left untouched.

The extension currently targets the Godot 4.3 ABI and has been exercised with
Godot 4.7.2 Mono on macOS arm64. Distribution remains subject to the licenses
of `gd_cubism`, the Cubism Native SDK, and the sample model.

## Layout

- `addons/gd_cubism/` — generated local copy of the addon used at runtime.
- `assets/live2d/` — ignored local model assets.
- `tests/` — smoke, ordering, mask, and render tests.

Performance benchmarks are intentionally kept out of this interactive project.
See `benchmarks/cubism-matrix/` for the six Core/runtime combinations.
