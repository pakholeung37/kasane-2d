# Native validation

Run these commands from the repository root. The root CMake presets include all
Kasane source, codec, package and Purism unit tests. They do not need the
proprietary Cubism SDK or external model fixtures.

```sh
cmake --preset core-debug
cmake --build --preset core-debug --parallel
ctest --preset core-debug
```

The `core-asan` preset runs the same tests with ASan and UBSan. The
`memory-core` preset excludes the PNG-backed package when libpng is unavailable.
On hosts without Ninja, use `core-make` with the same configure/build/test
commands.
CI runs GCC Debug and Clang ASan/UBSan on Linux. Warnings are enabled for owned
native targets and the Godot adapter, and are treated as errors. Third-party
Godot and Purism headers are marked as system headers for the adapter so their
own diagnostics do not fail this gate.

Format and static analysis use **clang-format and clang-tidy 22.1.8**. CI
installs exact wheel versions and the validation script rejects another
version. The root `.clang-format` selects LLVM as the default style, with
4-space indentation and blank lines between definitions. Nested upstream
projects keep their own style files. On macOS, Homebrew `llvm@22` provides
both tools and libFuzzer. Run:

```sh
cmake --preset core-debug
target/kasane/buildenv/bin/python -m SCons -C modules/gd-kasane platform=macos arch=arm64 target=template_debug compiledb
python3 tools/check_cpp_quality.py
```

Use `--compile-commands target/cmake/core-make` when using the Makefiles preset.
The format gate checks 109 owned C/C++ files, excluding generated and vendor
sources. Any formatting edit required by clang-format fails the gate.
The selected tidy defect checks cover 23 Kasane and Godot translation units.
Its two compilation databases come from CMake and SCons.

`validation_negative_controls` is part of CTest. It requires zero cases,
missing references, nonfinite numbers, wrong values and failed children to
produce a nonzero exit. Purism external-model conformance is opt-in with
`PURISM_CORE_EXTERNAL_CONFORMANCE=ON`; without local model and reference data,
it is **not run** and must not be reported as passed.

The reusable regression runners are `tools/validate_core.py`,
`tools/validate_godot.py` and `tools/validate_gpu.py`. Run them separately on
the supported macOS setup for official Core compatibility, Godot integration
and GPU comparisons. Their reports live under `target/kasane/core-regression/`,
`target/kasane/godot-boundary/` and `target/kasane/gpu-regression/`.
CTest also checks the Purism C99 bundle. The CMake native suite is the fast
development gate; Godot and GPU checks require their own runtime environment.

On a host with Clang's libFuzzer runtime, run bounded fuzz checks with:

```sh
sh tools/run_purism_fuzz.sh arena 1000
sh tools/run_purism_fuzz.sh malloc 1000
```

Each mode has its own library objects, harness and mutable corpus in `target`.
The versioned seed corpus lives in `modules/purism-core/fuzz/corpus`. Reproduce
a crash by passing its saved input file to the same mode's `purism_fuzzer`.
The runner uses Homebrew LLVM 22 on macOS and clang-18 on Linux CI. Apple's
Command Line Tools Clang does not supply a usable runtime on this host.

For source edits, run `core-debug`. For public headers, CMake, validator or
memory management edits, also run `core-asan`. Godot adapter edits require the
headless boundary check, and renderer edits require the GPU regression check
on the supported machine. Run the full suite periodically even when a change
appears isolated.
