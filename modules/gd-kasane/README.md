# gd-kasane

Internal Godot bindings for Kasane Editor, not a user-installed addon.

- `KasaneDocumentBridge`: RefCounted source-data owner independent of the scene
  tree; stable-ID Mesh/legacy-Deformer handles carry a generation.
- `KasaneDocumentPreview`: Node2D consuming DrawableFrame, including order,
  multiply/screen color, three blends and ordinary/inverted alpha masks.
- `KasaneTextureStore`: loaded Texture2D resources, separate from source metadata.
- `KasaneProjectIO`: source snapshots without loading textures or creating nodes.
  Format 5 persists all M1 fields; versions 1–4 are explicitly rejected. This
  does not certify M2 packaged-project relocation or migration.

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
python3 -m SCons -C modules/gd-kasane platform=macos arch=arm64 target=template_debug -j8
python3 tools/validate_godot.py
# Requires the existing gd-cubism reference build and Pillow/numpy:
target/kasane/buildenv/bin/python tools/validate_gpu.py
```

The unified [M1 acceptance](../../docs/editor/M1-ACCEPTANCE.md) command rebuilds
both libraries and runs the data and GPU gates. It records the existing Godot
first-dynamic-import shutdown crash separately, and preregisters extensions.
Editor packaging and full script-interface acceptance remain in M5.
