# gd-kasane

Internal Godot bindings for Kasane Editor, not a user-installed addon.

- `KasaneDocumentBridge`: RefCounted source-data owner independent of the scene
  tree; stable-ID Mesh/legacy-Deformer handles carry a generation.
- `KasaneDocumentPreview`: Node2D consuming DrawableFrame, including order,
  multiply/screen color, three blends and ordinary/inverted alpha masks.
- `KasaneTextureStore`: loaded Texture2D resources, separate from source metadata.
- `KasaneProjectIO`: portable directory projects, verified PNG assets, save-as,
  resource diagnostics and runtime package export, independent of scene nodes.
  `kasane-directory-project` version 1 replaces the explicitly rejected experimental
  formats. See [M2 format](../../docs/milestones/M2-FORMAT.md) and
  [acceptance](../../docs/milestones/M2-ACCEPTANCE.md).

Source changes and temporary preview changes have separate signals. Preview
values do not dirty or persist source data. Preview failures do not roll back
successful source edits. Formal Parts, Transforms and SceneBindings have typed
core APIs and dictionary adapters; geometry replacement preserves drawing
properties. Position-only Keyform edits preserve the other animated channels.

The current preview uses a simple shader/material adapter and per-mask viewports;
M4 will extract the existing Cubism renderer's parameter textures/atlas away from
its CubismModel owner and replace this temporary adapter. Both paths are checked
against identical GPU thresholds during M1 acceptance.

```sh
# Requires libpng, OpenSSL and pkg-config in addition to the compiler and SCons.
python3 -m SCons -C modules/gd-kasane platform=macos arch=arm64 target=template_debug -j8
python3 tools/validate_godot.py
# Requires freshly built Core probes, the official SDK, and a real GPU:
python3 tools/validate_project.py
# Requires the existing gd-cubism reference build and Pillow/numpy:
target/kasane/buildenv/bin/python tools/validate_gpu.py
```

The validation scripts build or exercise the data, persistence and GPU gates. It records the existing Godot
first-dynamic-import shutdown crash separately, and preregisters extensions.
Editor packaging and full script-interface acceptance remain in M5.

Persistence lives in [kasane-document](../kasane-document/README.md). The bridge owns a native DocumentSession; it only converts script arguments/results and resource pixels. All production file APIs require native absolute paths. FileAccess, res:// and user:// are not persistence backends.
