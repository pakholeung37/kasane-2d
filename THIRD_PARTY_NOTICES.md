# Third-party notices

The repository-level MIT License applies only where a file, directory,
dependency, or submodule does not carry a different notice. The following
upstream works retain their original copyrights and license terms.


## GDCubism

Portions of `modules/gd-cubism` are derived from
[GDCubism](https://github.com/MizunagiKB/gd_cubism).

Copyright (c) 2023 MizunagiKB.

Those portions are used under the MIT License. GDCubism's MIT license does not
cover the Live2D libraries with which it may be built or linked.

## godot-cpp

`modules/gd-cubism/godot-cpp` is a Git submodule of
[godot-cpp](https://github.com/godotengine/godot-cpp) and remains subject to
the license distributed within that submodule.

## PurismCore

`modules/purism-core` is a forked Git submodule of
[PurismCore](https://github.com/SakuraMotion/PurismCore).

Copyright (c) 2026 Sakura Motion Project.

PurismCore is distributed under the MIT License included in that submodule.

## undoredo

The first-write delta recording and reversible-edit design in
`modules/kasane-core/src/history.rs` is adapted from
[mikwielgus/undoredo](https://github.com/mikwielgus/undoredo), version 0.15.1,
commit `70306463634a5f2723333a8b2c30acaf98bac041`.

Copyright (c) 2025-2026 undoredo contributors.
Used under the MIT option of its MIT OR Apache-2.0 license.
The upstream MIT license text is included in [licenses/undoredo-MIT.txt](licenses/undoredo-MIT.txt).
The adaptation specializes recording for Kasane fields, uses value swapping
for playback, and adds bounded linear history; it does not depend on the
upstream crate, maplike, or its derive macros.
