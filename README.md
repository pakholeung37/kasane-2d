# Kasane 2D

Kasane 2D provides a Rust authoring SDK, Python bindings, and a WGPU renderer
for editable 2D models. The current workflow creates or imports a project with
`kasane.Session`, edits it in Python, observes rendered frames with
`kasane.Observer`, and exports a MOC3 package.

See [architecture](docs/ARCHITECTURE.md), [SDK API](docs/SDK-API.md), and
[validation](docs/VALIDATION.md) for the current interfaces and checks. The
[model inventory](models/README.md) records local and SDK sample models used by
tests and benchmarks.

## Build and test

```sh
cargo test --workspace --locked
RUSTFLAGS='-C strip=none' python3.14 -m pip wheel --no-deps --wheel-dir target/wheels modules/kasane-python
python3.14 tools/validate_sdk.py --wheel /absolute/path/to/kasane.whl
```

The wheel validator installs the wheel into a temporary environment outside
the source tree and tests project creation, import, editing, save, and export.
An observe-enabled wheel can also run GPU and pinned image-reference checks;
`--full` additionally requires official and Purism Core probes. See the
[tool guide](tools/README.md) for the complete commands.

## Acknowledgements

This project builds on upstream open-source projects:

- [GDCubism](https://github.com/MizunagiKB/gd_cubism) by MizunagiKB remains in
  the separate Cubism comparison benchmark. GDCubism-derived portions
  remain Copyright (c) 2023 MizunagiKB under the MIT License.
- [PurismCore](https://github.com/SakuraMotion/PurismCore) by the Sakura Motion
  Project is included through a forked Git submodule as an alternative Cubism
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
[third-party notices](THIRD_PARTY_NOTICES.md) for their scopes.

Live2D, Cubism, the Live2D Cubism SDK, Cubism Core, Cubism Native Framework,
and associated sample data are owned by or licensed through Live2D Inc. and/or
their respective rightsholders. This project is independent and is not
affiliated with, authorized by, endorsed by, or sponsored by Live2D Inc. The
names “Live2D” and “Cubism” describe interoperability only.

This repository does not distribute the proprietary Cubism Core binary, a
Cubism SDK package, or Live2D sample model assets. Users who obtain, build,
link, publish, or distribute software using Live2D materials are responsible
for complying with all applicable terms, including the
[Live2D Proprietary Software License Agreement](https://www.live2d.com/eula/live2d-proprietary-software-license-agreement_en.html),
[Live2D Open Software License Agreement](https://www.live2d.com/eula/live2d-open-software-license-agreement_en.html),
and any applicable [sample data terms](https://www.live2d.com/eula/live2d-sample-model-terms_en.html).
The MIT License for this repository grants no rights to third-party software,
models, artwork, trademarks, or other materials.
