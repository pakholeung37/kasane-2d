# Kasane 2D

Kasane 2D is building a Godot-based, agent-first Live2D editor with an editable
Document, project persistence, GDScript authoring, and MOC3 import and export.
The repository already includes a PurismCore + gd-cubism runtime for loading,
deforming, and displaying models. Editor delivery is tracked in the archived
[engineering roadmap](docs/archive/ROADMAP.md) and its separate milestones.
Native build and test commands are in [native validation](docs/archive/NATIVE-VALIDATION.md).

## Desktop application shells

Standalone Editor and Viewer entry points are available under `apps/`.
See [startup, checks and export instructions](apps/README.md). Model editing and
runtime loading are not connected yet; these shells do not complete M5/M6.

## Acknowledgements

This project builds on the work of upstream open-source projects:

- [GDCubism](https://github.com/MizunagiKB/gd_cubism) by MizunagiKB, which
  provides the foundation of the Godot integration. GDCubism-derived portions
  remain Copyright (c) 2023 MizunagiKB and are used under the MIT License.
- [PurismCore](https://github.com/SakuraMotion/PurismCore) by the Sakura Motion
  Project, included through a forked Git submodule as an alternative Cubism
  Core-compatible provider under its MIT License.

The maintainers and contributors of those projects are not responsible for,
and do not necessarily endorse, the changes made in this repository.

## License and third-party rights

Original code and modifications in this repository are available under the
[MIT License](LICENSE), except where a file, directory, dependency, or
submodule carries a different notice. Existing third-party copyright and
license notices remain in force. In particular, the `godot-cpp` submodule and
Live2D-derived benchmark sources are governed by their respective licenses;
the repository MIT License does not relicense them. See
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for the applicable scopes and
notices.

Live2D, Cubism, the Live2D Cubism SDK, Cubism Core, Cubism Native Framework,
and associated sample data are owned by or licensed through Live2D Inc. and/or
their respective rightsholders. This project is independent and is not
affiliated with, authorized by, endorsed by, or sponsored by Live2D Inc. The
names “Live2D” and “Cubism” are used only to describe interoperability; no
affiliation or endorsement is implied.

This repository does not distribute the proprietary Cubism Core binary, a
Cubism SDK package, or Live2D sample model assets. Users who obtain, build,
link, publish, or distribute software using Live2D materials are responsible
for complying with all applicable terms, including the
[Live2D Proprietary Software License Agreement](https://www.live2d.com/eula/live2d-proprietary-software-license-agreement_en.html),
[Live2D Open Software License Agreement](https://www.live2d.com/eula/live2d-open-software-license-agreement_en.html),
and any applicable [sample data terms](https://www.live2d.com/eula/live2d-sample-model-terms_en.html).
The MIT License for this repository grants no rights to third-party software,
models, artwork, trademarks, or other materials.
