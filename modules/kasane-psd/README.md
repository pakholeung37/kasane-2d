# PSD import module

`kasane-psd` converts a layered, 8-bit RGB PSD into a new in-memory Kasane `Document` and a list of PNG assets. It does not write files or replace a live editing session.

```rust
let bundle = kasane_psd::import_psd(&std::fs::read("art.psd")?)?;
// Publish each bundle.assets[i].bytes at bundle.assets[i].source,
// relative to the destination project root, before opening/saving the document.
let document = bundle.document;
```

Raster layers become cropped PNG assets and rectangular meshes. The PSD canvas, layer names, positions, stacking order, visibility, opacity, supported blend modes, and group hierarchy are retained. A raster base and its consecutive clipping layers become an isolated offscreen group. Clipped layers use source-atop blending, preserving the base's pixel alpha, opacity, and visibility; the base's blend mode applies to the completed group. Synthetic clipping Parts retain the individual editable meshes and do not increase the report's PSD group count. Object IDs and PNG paths are deterministic for identical input bytes. This module does not infer a rig, parameters, or deformers.

Current supported blend modes are normal, multiply, and linear dodge (additive). Clipped groups, clipping without a raster base in the same group, clipping with **Blend Clipped Layers As Group** explicitly disabled, layer masks, vector masks, adjustments, layer effects, group opacity, and unsupported blend modes return `UNSUPPORTED_LAYER`. Clipping uses offscreens and extended blends, requiring MOC3 5.3 when exporting. Text, vector, and placed layers with available raster pixels import as raster images and produce warnings. Empty or pixel-less layers are rejected. PSD/PSB features outside 8-bit RGB PSD version 1 are rejected. Input and decoded-image limits guard memory use.

The returned asset paths are relative. `kasane-project` publishes the PNG files and manifest together through `DocumentSession::import_psd_authoring`; `kasane-sdk` and Python expose this as `import_psd(source, destination)`. The destination must be a new project directory. A failed import leaves the active project untouched.

For unsaved editing, Rust callers use `AuthoringSession::import_psd_in_memory(source, expected_version)` and read textures with `read_asset(asset_id)`. PNGs remain in the session's memory until `save_project` is called; import writes no project or temporary resource files. The first save retains undo/redo, including original imported textures that appear only in history.
